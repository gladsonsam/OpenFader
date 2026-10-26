use super::*;

#[test]
fn strip_keys_survive_desk_switch() {
    // Positional indices collide across desks (sim strip 32 = first bus
    // master, xair strip 32 = third DCA); stable keys do not.
    use openfader_core::{strip_index_for_key, strip_key_for_index};
    let engine = Engine::new(EngineConfig::default());
    let sim = engine.driver_topology("sim");
    let xair = engine.driver_topology("xair");
    // sim strip 32 keys as the first bus master on both desks.
    let k = strip_key_for_index(&sim, 32).expect("sim strip 32 keys");
    assert_eq!(k, "bus/01");
    assert_eq!(strip_index_for_key(&xair, &k), Some(20));
    // The same index means different strips per desk.
    assert_eq!(strip_key_for_index(&xair, 32), Some("dca/02".to_string()));
    // The sim `mix/..` alias resolves on xair too.
    assert_eq!(strip_index_for_key(&xair, "mix/01"), Some(20));
    // Main is bare on every desk.
    assert_eq!(strip_key_for_index(&sim, 72), Some("main".to_string()));
    assert_eq!(strip_index_for_key(&xair, "main"), Some(30));
    // Unknown families miss instead of aliasing a wrong strip.
    assert_eq!(strip_index_for_key(&xair, "bus/07"), None);
    assert_eq!(strip_index_for_key(&sim, "grp/01"), None);
}

#[test]
fn recall_scope_matches_strip_keys() {
    // Union semantics: indices OR resolved keys, unknown keys ignored.
    let engine = Engine::new(EngineConfig::default());
    let sim = engine.driver_topology("sim");
    let all = RecallScope::default();
    assert!(all.matches_strip(0, &sim));
    let by_idx = RecallScope {
        strips: Some(vec![1]),
        ..Default::default()
    };
    assert!(by_idx.matches_strip(1, &sim));
    assert!(!by_idx.matches_strip(0, &sim));
    let by_key = RecallScope {
        strip_keys: Some(vec!["ch/02".to_string()]),
        ..Default::default()
    };
    assert!(by_key.matches_strip(1, &sim));
    assert!(!by_key.matches_strip(0, &sim));
    // Legacy alias participates in the union.
    let by_alias = RecallScope {
        strips: Some(vec![0]),
        strip_keys: Some(vec!["mix/01".to_string()]),
        ..Default::default()
    };
    assert!(by_alias.matches_strip(0, &sim));
    assert!(by_alias.matches_strip(32, &sim));
    assert!(!by_alias.matches_strip(2, &sim));
    // Unknown keys never match (another desk's strips, not a failure).
    let unknown = RecallScope {
        strip_keys: Some(vec!["grp/01".to_string()]),
        ..Default::default()
    };
    assert!(!unknown.matches_strip(0, &sim));
}

#[test]
fn old_frames_parse_without_keys() {
    // Wire back-compat: every key-bearing frame parses when `key` (and
    // `strip_keys` / `strip_keys`) is absent, as the committed golden
    // fixtures prove.
    for text in [
        r#"{"type":"state","driver":"sim","strips":[{"ch":0,"name":"CH01","fader":0.5,"muted":false}],"main_fader":0.8}"#,
        r#"{"type":"param","ch":2,"field":"gain","value":12.5}"#,
        r#"{"type":"params","ch":3,"values":{"gain":12.5}}"#,
        r#"{"type":"solo","ch":1,"soloed":true}"#,
        r#"{"type":"subgroup_assign","ch":0,"group":1,"member":true}"#,
        r#"{"type":"mute_group_assign","ch":5,"group":2,"member":false}"#,
        r#"{"type":"bus","bus":0,"fader":0.75,"muted":false}"#,
        r#"{"type":"subgroup","group":1,"fader":0.7,"muted":true}"#,
        r#"{"type":"mute_group","group":0,"muted":true}"#,
        r#"{"type":"routing","patches":[{"strip":0,"group":"Net","index":5,"label":"Net 05"}]}"#,
        r#"{"type":"offline_diff","changes":[{"strip":0,"id":"gain","from":20.0,"to":25.0}],"count":1}"#,
        r#"{"type":"caps","driver":"sim","params":[],"unsupported":[],"topology":[{"key":"ch","kind":"in","label":"Ch","first":0,"count":2,"name_prefix":"Ch","color_default":2,"params_scope":["in"]}]}"#,
    ] {
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let evt: Event =
            serde_json::from_value(v).unwrap_or_else(|e| panic!("old frame parses: {text}: {e}"));
        let back = serde_json::to_value(&evt).unwrap();
        assert!(
            back.get("key").is_none(),
            "old frame stays key-less: {text}"
        );
    }
    // Old commands (no `key`) parse; new commands accept `key`.
    for text in [
        r#"{"cmd":"fader","ch":0,"value":0.5}"#,
        r#"{"cmd":"mute","ch":0,"muted":true}"#,
        r#"{"cmd":"param","ch":1,"field":"gain","value":12.5}"#,
        r#"{"cmd":"lock","ch":0}"#,
        r#"{"cmd":"patch_set","strip":0,"group":"Lcl","index":1}"#,
        r#"{"cmd":"subgroup_assign","ch":0,"group":1,"member":true}"#,
    ] {
        crate::protocol::parse_cmd(text).unwrap_or_else(|e| panic!("old cmd parses: {text}: {e}"));
    }
    let cmd =
        crate::protocol::parse_cmd(r#"{"cmd":"fader","ch":0,"key":"ch/02","value":0.5}"#).unwrap();
    assert!(matches!(cmd, Cmd::Fader { key: Some(_), .. }));
    // Recall scopes accept `strip_keys` alongside `strips`.
    let scope: RecallScope =
        serde_json::from_value(serde_json::json!({"strips":[0],"strip_keys":["ch/02"]})).unwrap();
    assert_eq!(scope.strip_keys, Some(vec!["ch/02".to_string()]));
    let legacy: RecallScope = serde_json::from_value(serde_json::json!({"strips":[0]})).unwrap();
    assert_eq!(legacy.strip_keys, None);
}

#[tokio::test]
async fn cmd_key_authoritative_over_index() {
    // A profile replayed on another desk addresses strips by key: the
    // key wins over the positional `ch`.
    let engine = parked_engine();
    let mut client = engine.attach(None).await;
    settle_parked_poller(&engine, &mut client).await;
    client
        .send(Cmd::Fader {
            ch: 0,
            key: Some("ch/02".to_string()),
            value: 0.5,
        })
        .await
        .unwrap();
    let mut saw: Option<(u16, Option<String>)> = None;
    for _ in 0..128 {
        match client.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Param { ch, key, field, .. } if field == "fader" => {
                    saw = Some((*ch, key.clone()));
                    break;
                }
                _ => continue,
            },
            Err(_) => break,
        }
    }
    assert_eq!(
        saw,
        Some((1, Some("ch/02".to_string()))),
        "key resolves to strip 1"
    );
    // Unknown keys are honest errors, never a silent wrong-strip write.
    client
        .send(Cmd::Mute {
            ch: 0,
            muted: true,
            key: Some("grp/01".to_string()),
        })
        .await
        .unwrap();
    let mut erred = false;
    for _ in 0..128 {
        match client.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Error { message, .. } if message.contains("grp/01") => {
                    erred = true;
                    break;
                }
                _ => continue,
            },
            Err(_) => break,
        }
    }
    assert!(erred, "unknown strip key errors honestly");
}

#[tokio::test]
async fn scene_recall_resolves_keys() {
    // A scene captured on one desk recalls at resolved keys: save with
    // strip 0 at 0.9, rewrite its key to ch/02, recall fades strip 1.
    let engine = parked_engine();
    let mut client = engine.attach(None).await;
    settle_parked_poller(&engine, &mut client).await;
    client
        .send(Cmd::SceneSave {
            n: 7,
            name: "keys".to_string(),
        })
        .await
        .unwrap();
    {
        let mut lib = engine.scenes_lib.write().unwrap();
        let scene = lib.get_mut(&7).expect("scene saved");
        // Drop the native ch/02 row so the re-keyed row is the only
        // writer to strip 1 (otherwise the later row would win).
        scene.strips.retain(|s| s.strip != 1);
        let row = scene
            .strips
            .iter_mut()
            .find(|s| s.strip == 0)
            .expect("strip 0 captured");
        row.fader = 0.9;
        row.key = Some("ch/02".to_string());
    }
    client
        .send(Cmd::SceneRecall {
            n: 7,
            fade_ms: Some(0),
            scope: None,
        })
        .await
        .unwrap();
    let drv = engine.drv("sim");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let v = drv
            .get_fader(openfader_core::StripId(1))
            .await
            .unwrap_or(0.0);
        if (v - 0.9).abs() < 1e-6 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("key-resolved recall never landed on strip 1");
        }
        tokio::task::yield_now().await;
    }
    // Scoped recall by key: only the key-named strip moves.
    client
        .send(Cmd::Fader {
            ch: 0,
            key: None,
            value: 0.1,
        })
        .await
        .unwrap();
    client
        .send(Cmd::SceneRecall {
            n: 7,
            fade_ms: Some(0),
            scope: Some(RecallScope {
                strips: None,
                strip_keys: Some(vec!["ch/02".to_string()]),
                groups: None,
                globals: Some(vec![]),
            }),
        })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let v = drv
            .get_fader(openfader_core::StripId(1))
            .await
            .unwrap_or(0.0);
        if (v - 0.9).abs() < 1e-6 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("scoped key recall never landed");
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn inbound_solo_fans_out_solo_event() {
    // Desk-originated solo (ingest `solo` id, poll bank read) reaches
    // clients as `Event::Solo`, the channel the UI solos view
    // renders, while other params still ride `Params` frames.
    // Parked poller: after the settle only `apply_inbound` moves the
    // mailbox, so the sequence below is exact.
    use tokio::sync::mpsc::error::TryRecvError;
    let engine = parked_engine();
    let mut handle = engine.attach(None).await;
    settle_parked_poller(&engine, &mut handle).await;
    engine.apply_inbound(2, "solo", &ParamValue::Bool(true));
    let mut found = None;
    for _ in 0..64 {
        match handle.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Solo { ch, soloed, .. } => {
                    found = Some((*ch, *soloed));
                    break;
                }
                _ => continue,
            },
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    assert_eq!(found, Some((2, true)), "solo-in must fan out Solo");
    // A repeat of the same state stays quiet (poll diff semantics
    // no Solo echo storm every sweep).
    engine.apply_inbound(2, "solo", &ParamValue::Bool(true));
    let mut dup = false;
    for _ in 0..64 {
        match handle.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Solo { .. } => {
                    dup = true;
                    break;
                }
                _ => continue,
            },
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    assert!(!dup, "duplicate solo state must stay quiet");
    // Unsolo fans out, and the param plane is untouched by solos.
    engine.apply_inbound(2, "solo", &ParamValue::Bool(false));
    let mut found = None;
    for _ in 0..64 {
        match handle.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Solo { ch, soloed, .. } => {
                    found = Some((*ch, *soloed));
                    break;
                }
                _ => continue,
            },
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    assert_eq!(found, Some((2, false)), "solo-out must fan out Solo");
    engine.apply_inbound(2, "fader", &ParamValue::Float(0.5));
    let mut saw_params = false;
    for _ in 0..64 {
        match handle.events.try_recv() {
            Ok(frame) => match &*frame {
                Event::Params { ch: 2, .. } => {
                    saw_params = true;
                    break;
                }
                _ => continue,
            },
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    assert!(saw_params, "non-solo params still ride Params frames");
}
