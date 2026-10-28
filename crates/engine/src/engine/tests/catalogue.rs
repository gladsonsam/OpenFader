use super::*;

#[tokio::test]
async fn caps_sim_lists_full_catalogue() {
    let engine = Engine::new(EngineConfig::default());
    let caps = event_json(engine.caps_message());
    assert_eq!(caps.get("type"), Some(&json!("caps")));
    assert_eq!(caps.get("driver"), Some(&json!("sim")));
    let params = caps.get("params").and_then(Value::as_array).unwrap();
    assert_eq!(params.len(), engine.registry("sim").len());
    let ids: Vec<&str> = params
        .iter()
        .map(|p| p.get("id").and_then(Value::as_str).unwrap())
        .collect();
    for def in openfader_core::params::CATALOGUE {
        assert!(ids.contains(&def.id), "caps miss {}", def.id);
    }
    // Every entry carries the pinned frame shape, v2 scopes included
    // (legacy `scope` kept for back-compat).
    for p in params {
        for key in [
            "id", "kind", "scope", "scopes", "min", "max", "unit", "default", "label",
        ] {
            assert!(p.get(key).is_some(), "caps entry misses {key}: {p}");
        }
    }
    // `caps.topology` covers the sim strip space; `meter_taps` names the
    // declared taps (additive: old fields byte-identical).
    let topo = caps.get("topology").and_then(Value::as_array).unwrap();
    assert_eq!(topo.len(), openfader_sim::SIM_TOPOLOGY.len());
    let first = &topo[0];
    assert_eq!(first.get("key"), Some(&json!("ch")));
    assert_eq!(first.get("kind"), Some(&json!("in")));
    assert_eq!(first.get("first"), Some(&json!(0)));
    assert_eq!(first.get("count"), Some(&json!(32)));
    for key in [
        "key",
        "kind",
        "label",
        "first",
        "count",
        "name_prefix",
        "color_default",
        "params_scope",
    ] {
        assert!(
            first.get(key).is_some(),
            "topology entry misses {key}: {first}"
        );
    }
    assert_eq!(
        caps.get("meter_taps"),
        Some(&json!(["in", "post", "gr_gate", "gr_comp"]))
    );
    assert_eq!(caps.get("unsupported"), Some(&json!([])));
}

#[test]
fn driver_capability_sets() {
    let engine = Engine::new(EngineConfig::default());
    // sim = full catalogue; xair = loopback-tested subset; dl32s =
    // Wave-1 verified set (see `openfader_proto_dl32s::DL32S_CAPABILITIES`).
    assert_eq!(
        engine.driver_capabilities("sim").len(),
        engine.registry("sim").len()
    );
    assert_eq!(
        engine.driver_capabilities("xair"),
        openfader_proto_xair::XAIR_CAPABILITIES
    );
    assert_eq!(
        engine.driver_capabilities("dl32s"),
        openfader_proto_dl32s::DL32S_CAPABILITIES
    );
    assert_eq!(
        engine.driver_capabilities("nope"),
        engine.driver_capabilities("sim")
    );
    assert_eq!(engine.driver_channels("xair"), 16);
    assert_eq!(engine.driver_channels("sim"), 32);
    // Strip totals (topology): sim 77, xair 36, dl32s 44.
    assert_eq!(engine.driver_strips("sim"), openfader_sim::SIM_STRIPS);
    assert_eq!(
        engine.driver_strips("xair"),
        openfader_proto_xair::XAIR_STRIPS
    );
    assert_eq!(engine.driver_strips("dl32s"), 44);
    assert_eq!(
        engine.driver_topology("sim").len(),
        openfader_sim::SIM_TOPOLOGY.len()
    );
    assert_eq!(
        engine.driver_topology("xair").len(),
        openfader_proto_xair::XAIR_TOPOLOGY.len()
    );
    assert_eq!(
        engine.driver_meter_taps("sim"),
        vec!["in", "post", "gr_gate", "gr_comp"]
    );
}

#[test]
fn registries_merge_canonical_plus_provider_rows() {
    use openfader_core::params::ParamRegistry;
    let engine = Engine::new(EngineConfig::default());
    // Provider rows append without replacing the canonical table.
    for driver in ["sim", "xair", "dl32s"] {
        let reg = engine.registry(driver);
        assert_eq!(
            reg.len(),
            openfader_core::params::CATALOGUE.len() + engine.drv(driver).param_defs().len(),
            "{driver}"
        );
        for def in openfader_core::params::CATALOGUE {
            assert_eq!(reg.find(def.id).map(|d| d.id), Some(def.id), "{driver}");
        }
        // Canonical fader/mute/name/pan stable through the merge.
        for id in ["fader", "mute", "name", "pan"] {
            assert!(reg.find(id).is_some(), "{driver} misses {id}");
        }
    }
    // Unknown drivers fall back to the sim registry (mirrors `drv`).
    assert_eq!(engine.registry("nope").len(), engine.registry("sim").len());
    // The merge helper itself: canonical wins, provider rows append.
    let mut reg = ParamRegistry::canonical();
    reg.register(engine.drv("xair").param_defs());
    assert_eq!(
        reg.len(),
        openfader_core::params::CATALOGUE.len() + engine.drv("xair").param_defs().len()
    );
}

/// Resume path: `push_snapshot` queues the attach frame set into an
/// already-attached client's mailbox (fresh state, no `you`, no roster
/// churn, no lock/role change). This is what the native shell calls when
/// the app comes back from the background, so a UI that missed minutes
/// of frames catches up in one batch.
#[tokio::test]
async fn push_snapshot_refreshes_without_reattaching() {
    let engine = Engine::new(EngineConfig::default());
    let mut client = engine.attach(Some("resume-ui".to_string())).await;
    let id = client.id.clone();
    // Drain the attach snapshot, then warm the param cache (a fresh
    // snapshot only carries `params` for cached channels).
    while client.events.try_recv().is_ok() {}
    for _ in 0..20 {
        engine.poll_once().await;
    }
    while client.events.try_recv().is_ok() {}
    engine.push_snapshot(&id).await;
    let mut tags: Vec<String> = Vec::new();
    while let Ok(event) = client.events.try_recv() {
        tags.push(
            shared_json(&event)
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string(),
        );
    }
    for want in [
        "state", "shows", "clients", "locks", "caps", "devices", "link", "scenes", "fx", "routing",
        "params",
    ] {
        assert!(
            tags.contains(&want.to_string()),
            "resume snapshot carries `{want}` (got {tags:?})"
        );
    }
    assert!(
        !tags.contains(&"you".to_string()),
        "resume snapshot must not re-identify the client (got {tags:?})"
    );
    // No re-attach: same id, exactly one roster entry.
    assert_eq!(client.id, id);
    {
        let clients = engine.clients.read().unwrap();
        assert_eq!(clients.len(), 1, "resume must not duplicate the client");
        assert!(clients.contains_key(&id));
    }
    // Unknown id: no-op (no panic, no mailbox created).
    engine.push_snapshot("client-does-not-exist").await;
    assert_eq!(engine.clients.read().unwrap().len(), 1);
}

#[tokio::test]
async fn dropout_shrinks_caps_once() {
    let engine = Engine::new(EngineConfig::default());
    let mut client = engine.attach(None).await;
    // Drain the attach snapshot (R1 six-frame prefix, then the R2
    // frames: devices/link/info/scenes/fx/routing + the master-section
    // snapshot: sim serves 8 bus + 6 subgroup + 6 mute-group masters +
    // main, no assigns/solos on a fresh engine).
    let mut tags: Vec<String> = Vec::new();
    for _ in 0..33 {
        let event = tokio::time::timeout(Duration::from_secs(2), client.events.recv())
            .await
            .expect("snapshot in time")
            .expect("mailbox open");
        tags.push(
            shared_json(&event)
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string(),
        );
    }
    assert_eq!(
        &tags[..6],
        ["you", "state", "shows", "clients", "locks", "caps"],
        "R1 snapshot prefix unchanged"
    );
    assert_eq!(
        &tags[6..12],
        ["devices", "link", "info", "scenes", "fx", "routing"],
        "R2 snapshot frames"
    );
    assert_eq!(
        &tags[12..20],
        ["bus", "bus", "bus", "bus", "bus", "bus", "bus", "bus"],
        "attach carries all sim bus masters"
    );
    assert_eq!(
        &tags[20..26],
        ["subgroup", "subgroup", "subgroup", "subgroup", "subgroup", "subgroup"],
        "attach carries all sim subgroup masters"
    );
    assert_eq!(
        &tags[26..32],
        [
            "mute_group",
            "mute_group",
            "mute_group",
            "mute_group",
            "mute_group",
            "mute_group"
        ],
        "attach carries all sim mute-group masters"
    );
    assert_eq!(&tags[32..], ["main"], "attach ends masters with Main LR");
    // Timeout-class read: tolerated twice (one lost datagram on venue
    // wifi must not evict a live param), dropped on the third
    // consecutive strike and reported.
    while client.events.try_recv().is_ok() {} // membership false rows follow the legacy prefix
    let err = anyhow::anyhow!("/ch/01/eq/on: timed out after 2s");
    assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
    assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
    assert!(engine.note_poll_error("xair", "eq_on", &err).await);
    assert!(engine
        .param_dropped
        .read()
        .unwrap()
        .contains(&("xair".to_string(), "eq_on".to_string())));
    // A post-drop timeout starts a fresh count (strikes cleared on
    // eviction): no caps re-broadcast, no double report.
    assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
    // A success between strikes resets the count: two strikes, one ok,
    // two more strikes must NOT drop.
    assert!(!engine.note_poll_error("xair", "gain", &err).await);
    assert!(!engine.note_poll_error("xair", "gain", &err).await);
    engine.note_poll_ok("xair", "gain");
    assert!(!engine.note_poll_error("xair", "gain", &err).await);
    assert!(!engine.note_poll_error("xair", "gain", &err).await);
    assert!(!engine
        .param_dropped
        .read()
        .unwrap()
        .contains(&("xair".to_string(), "gain".to_string())));
    // Unknown params still drop on the first failure (no tolerance for
    // addresses the desk can never answer).
    let unknown = anyhow::anyhow!("xair: unknown param \"nope\"");
    assert!(engine.note_poll_error("xair", "nope", &unknown).await);
    // Other drivers are unaffected.
    assert!(!engine
        .param_dropped
        .read()
        .unwrap()
        .contains(&("sim".to_string(), "eq_on".to_string())));
    // Two caps updates went out on the fanout (one per eviction).
    for _ in 0..2 {
        let msg = tokio::time::timeout(Duration::from_secs(2), client.events.recv())
            .await
            .expect("caps update in time")
            .expect("mailbox open");
        assert_eq!(shared_json(&msg).get("type"), Some(&json!("caps")));
    }
    // No-data errors never drop.
    let nodata = anyhow::anyhow!("sim: gain has no value yet");
    assert!(!engine.note_poll_error("sim", "gain", &nodata).await);
    assert!(engine.param_dropped.read().unwrap().len() == 2);
}

#[tokio::test]
async fn poll_list_is_channel_major() {
    let engine = Engine::new(EngineConfig::default());
    for driver in ["sim", "xair", "dl32s"] {
        let list = engine.poll_list(driver).await;
        assert!(!list.is_empty(), "poll list for {driver}");
        // Strips non-decreasing: every param of strip N precedes
        // strip N+1, so one strip's whole set arrives as a block.
        let mut last_ch: u16 = 0;
        for (ch, _) in &list {
            assert!(*ch >= last_ch, "poll list not strip-major for {driver}");
            last_ch = *ch;
        }
        // Each strip carries the driver's supported id set filtered by
        // catalogue scopes (DCA strips: fader/mute/name/color only), in
        // capabilities order. Strips covered: the whole topology.
        let topo = engine.driver_topology(driver);
        let total: usize = topo.iter().map(|f| f.count).sum();
        let mut expect = 0usize;
        for fam in &topo {
            let n = engine
                .driver_capabilities(driver)
                .iter()
                .filter(|id| {
                    engine
                        .registry(driver)
                        .find(id)
                        .map(|def| openfader_core::params::supports_kind(def, fam.kind))
                        .unwrap_or(true)
                })
                .count();
            // Solo rides the poll set as its own id on strips with a
            // provider extra (see `static_poll_list`): one extra entry
            // per strip of the matching families.
            let n = n + engine.drv(driver).essentials_extra_ids(fam.kind).len();
            expect += fam.count * n;
            for s in fam.first..fam.first + fam.count {
                let ids: Vec<&str> = list
                    .iter()
                    .filter(|(c, _)| *c == s as u16)
                    .map(|(_, id)| id.as_str())
                    .collect();
                let mut want: Vec<&str> = engine
                    .driver_capabilities(driver)
                    .iter()
                    .filter(|id| {
                        engine
                            .registry(driver)
                            .find(id)
                            .map(|def| openfader_core::params::supports_kind(def, fam.kind))
                            .unwrap_or(true)
                    })
                    .copied()
                    .collect();
                want.extend(
                    engine
                        .drv(driver)
                        .essentials_extra_ids(fam.kind)
                        .iter()
                        .copied(),
                );
                assert_eq!(ids, want, "strip {s} carries the scoped set for {driver}");
            }
        }
        assert_eq!(list.len(), expect, "poll list size for {driver}");
        let max_strip = list.iter().map(|(c, _)| *c).max().unwrap() as usize;
        assert_eq!(
            max_strip + 1,
            total,
            "poll list covers the topology for {driver}"
        );
    }
}

#[tokio::test]
async fn poll_list_carries_xair_solo_bank() {
    // Desk→app solo backstop: xair channel strips carry the `solo`
    // poll id (bank re-read via `read_param`); DL32S also supplies it.
    // solo is not a catalogue param and never enters `caps`.
    let engine = Engine::new(EngineConfig::default());
    let list = engine.poll_list("xair").await;
    let nch = engine.driver_channels("xair");
    assert!(nch > 0, "xair has channel strips");
    for ch in 0..nch as u16 {
        assert!(
            list.contains(&(ch, "solo".to_string())),
            "xair ch{ch} misses the solo poll id"
        );
    }
    for (ch, id) in &list {
        if id == "solo" {
            assert!(
                (*ch as usize) < nch,
                "solo poll id off-channel: ch{ch} (channels 0..{nch})"
            );
        }
    }
    for driver in ["sim"] {
        let list = engine.poll_list(driver).await;
        assert!(
            !list.iter().any(|(_, id)| id == "solo"),
            "{driver} must not poll solo"
        );
    }
}

#[test]
fn wing_caps_name_matrix_and_main_send_destinations_and_scope_their_controls() {
    let engine = Engine::new(EngineConfig::default());
    *engine.active.write().unwrap() = "wing".into();
    let Event::Caps { params, .. } = engine.caps_message() else {
        panic!("caps frame expected")
    };
    let parameter = |id: &str| params.iter().find(|p| p.id == id).unwrap();
    for (id, key, label) in [
        ("send1_level", "bus/01", "Bus 1"),
        ("send17_level", "mtx/01", "Matrix 1"),
        ("send24_level", "mtx/08", "Matrix 8"),
        ("send25_level", "main/01", "Main 1"),
        ("send28_level", "main/04", "Main 4"),
    ] {
        let destination = parameter(id)
            .presentations
            .iter()
            .find_map(|p| p.get("destination"))
            .unwrap();
        assert_eq!(destination["key"], key);
        assert_eq!(destination["label"], label);
        assert_eq!(destination["stereo"], true);
        assert_eq!(destination["allow_self"], false);
    }
    assert!(parameter("send17_level").scopes.iter().any(|s| s == "main"));
    assert!(!parameter("send25_level").scopes.iter().any(|s| s == "main"));
    assert!(!parameter("send17_pre").scopes.iter().any(|s| s == "in"));
    assert_eq!(parameter("send28_level").min, json!(-144.0));
    let destination = engine.drv("wing").send_destination(29);
    assert!(destination.is_none());
    for (id, group) in [
        ("send7_level", "sends_bus"),
        ("send17_level", "sends_matrix"),
        ("send28_level", "sends_main"),
        ("lr_assign", "sends_main"),
        ("eq8_freq", "eq"),
        ("source_a", "source"),
    ] {
        let advertised = parameter(id)
            .presentations
            .iter()
            .find_map(|p| p.get("recall_group"))
            .unwrap();
        assert_eq!(advertised, group);
    }
}

#[tokio::test]
async fn wing_scoped_scene_recall_writes_only_the_selected_send_family() {
    // Exercise the real recall path and WING driver over UDP. Encode the
    // few fixture replies directly; no hardware or native meter service.
    fn osc_string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(value.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap().to_string();
    let writes = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = writes.clone();
    let task = tokio::spawn(async move {
        let mut data = [0u8; 4096];
        loop {
            let (n, peer) = socket.recv_from(&mut data).await.unwrap();
            let packet = openfader_transport::osc::decode_packet(&data[..n]).unwrap();
            for msg in openfader_transport::osc::flatten(packet) {
                if !msg.args.is_empty() {
                    log.lock().unwrap().push(msg.addr);
                    continue;
                }
                let mut reply = Vec::new();
                osc_string(&mut reply, &msg.addr);
                if msg.addr == "/?" || msg.addr.ends_with("/mdl") {
                    osc_string(&mut reply, ",s");
                    osc_string(
                        &mut reply,
                        if msg.addr == "/?" {
                            "WING,127.0.0.1,Scope fixture,wing-compact,TEST,3.0.5"
                        } else {
                            "NONE"
                        },
                    );
                } else if msg.addr.ends_with("/grp") || msg.addr.ends_with("/altgrp") {
                    osc_string(&mut reply, ",s");
                    osc_string(&mut reply, "LCL");
                } else if msg.addr.ends_with("/in/conn/in") || msg.addr.ends_with("/altin") {
                    osc_string(&mut reply, ",i");
                    reply.extend_from_slice(&3i32.to_be_bytes());
                } else if msg.addr.ends_with("/srcauto") {
                    osc_string(&mut reply, ",i");
                    reply.extend_from_slice(&1i32.to_be_bytes());
                } else if msg.addr.ends_with("/fdr") {
                    osc_string(&mut reply, ",f");
                    reply.extend_from_slice(&(-12f32).to_be_bytes());
                } else {
                    osc_string(&mut reply, ",i");
                    reply.extend_from_slice(&0i32.to_be_bytes());
                }
                socket.send_to(&reply, peer).await.unwrap();
            }
        }
    });
    let engine = Engine::new(EngineConfig::default());
    *engine.active.write().unwrap() = "wing".into();
    let drv = engine.drv("wing");
    drv.connect(&addr).await.unwrap();
    let scene = StoredScene {
        number: 1,
        name: "Sends".into(),
        saved_at: 0,
        main_fader: None,
        fx: vec![],
        routing: vec![],
        mute_groups: None,
        automix: None,
        output_taps: None,
        monitor: None,
        rta: None,
        output_eq: None,
        strips: vec![StoredStrip {
            strip: 0,
            key: Some("ch/01".into()),
            name: String::new(),
            fader: 0.5,
            mute: false,
            params: [1, 16, 17, 24, 25, 28]
                .into_iter()
                .map(|slot| (format!("send{slot}_on"), json!(true)))
                .collect(),
        }],
    };
    for (group, fields, paths) in [
        (
            "sends_bus",
            vec!["send1_on", "send16_on"],
            vec!["/ch/1/send/1/on", "/ch/1/send/16/on"],
        ),
        (
            "sends_matrix",
            vec!["send17_on", "send24_on"],
            vec!["/ch/1/send/MX1/on", "/ch/1/send/MX8/on"],
        ),
        (
            "sends_main",
            vec!["send25_on", "send28_on"],
            vec!["/ch/1/main/1/on", "/ch/1/main/4/on"],
        ),
        ("sends_fx", vec![], vec![]),
        ("config", vec![], vec![]),
        ("mute", vec!["mute"], vec!["/ch/1/mute"]),
        ("fader_pan", vec!["fader"], vec!["/ch/1/fdr"]),
    ] {
        writes.lock().unwrap().clear();
        let scope = RecallScope {
            groups: Some(vec![group.into()]),
            globals: Some(vec![]),
            ..Default::default()
        };
        let mut out = Vec::new();
        engine
            .recall_full(&scene, 0, Some(&scope), "test", &mut out)
            .await
            .unwrap();
        // Query barrier: all earlier loopback writes have reached the desk.
        drv.get_fader(StripId(0)).await.unwrap();
        let mut actual = writes.lock().unwrap().clone();
        actual.sort();
        let mut expected = paths;
        expected.sort();
        assert_eq!(actual, expected, "wire paths for {group}");
        let mut echoes: Vec<_> = out
            .iter()
            .filter_map(|event| match event {
                Event::Param { field, .. } => Some(field.clone()),
                _ => None,
            })
            .collect();
        echoes.sort();
        let mut expected: Vec<_> = fields.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(echoes, expected, "recall echoes for {group}");
    }
    let mut rejected = scene.clone();
    rejected.strips[0].params = HashMap::from([("wing:source_mode".into(), json!("invalid"))]);
    let scope = RecallScope {
        groups: Some(vec!["source".into()]),
        globals: Some(vec![]),
        ..Default::default()
    };
    let mut out = Vec::new();
    let error = engine
        .recall_full(&rejected, 0, Some(&scope), "test", &mut out)
        .await
        .unwrap_err();
    assert!(!out
        .iter()
        .any(|event| matches!(event, Event::Param { field, .. } if field == "wing:source_mode")));
    assert!(error
        .to_string()
        .contains("Snapshot recall incomplete: 1 setting(s)"));
    assert!(error.to_string().contains("wing:source_mode"));
    // Auto switching is active. Stored source context must reach the
    // guarded preamp write, even in an input-only scoped recall.
    let mut preamp = scene.clone();
    preamp.strips[0].params = HashMap::from([
        ("gain".into(), json!(20.)),
        ("source_a".into(), json!(1003.)),
        ("source_b".into(), json!(1003.)),
        ("source_select".into(), json!(0.)),
    ]);
    let scope = RecallScope {
        groups: Some(vec!["input".into()]),
        globals: Some(vec![]),
        ..Default::default()
    };
    writes.lock().unwrap().clear();
    let mut out = Vec::new();
    engine
        .recall_full(&preamp, 0, Some(&scope), "test", &mut out)
        .await
        .unwrap();
    drv.get_fader(StripId(0)).await.unwrap();
    assert_eq!(*writes.lock().unwrap(), vec!["/io/in/LCL/3/g"]);
    assert!(!out.iter().any(|event| matches!(event, Event::Error { .. })));
    preamp.strips[0]
        .params
        .insert("source_a".into(), json!(1004.));
    writes.lock().unwrap().clear();
    out.clear();
    let error = engine
        .recall_full(&preamp, 0, Some(&scope), "test", &mut out)
        .await
        .unwrap_err();
    drv.get_fader(StripId(0)).await.unwrap();
    assert!(writes.lock().unwrap().is_empty());
    assert!(!out
        .iter()
        .any(|event| matches!(event, Event::Param {field,..} if field == "gain")));
    assert!(error.to_string().contains("Input source changed"));
    drv.disconnect().await.unwrap();
    task.abort();
}
