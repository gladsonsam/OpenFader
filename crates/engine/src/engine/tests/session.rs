use super::super::models::send_slot_number;
use super::*;

#[test]
fn dial_addr_skips_sweep_only_for_bare_provider_ips() {
    // Discovered-device dial: a bare IP gains an explicit port when
    // the provider answers there (the sweep is skipped); everything
    // else passes through untouched.
    let xair = openfader_proto_xair::XAirDriver::new();
    let sim = openfader_sim::SimDriver::new();
    assert_eq!(
        crate::drivers::DynDriver::dial_addr(&xair, "192.168.1.20"),
        "192.168.1.20:10024"
    );
    assert_eq!(
        crate::drivers::DynDriver::dial_addr(&xair, "192.168.1.20:10024"),
        "192.168.1.20:10024"
    );
    assert_eq!(
        crate::drivers::DynDriver::dial_addr(&xair, "desk.local"),
        "desk.local"
    );
    assert_eq!(
        crate::drivers::DynDriver::dial_addr(&sim, "192.168.1.20"),
        "192.168.1.20"
    );
}

#[test]
fn poller_yield_rule() {
    // MR1 write priority: the poller skips its tick while ANY command
    // is inside apply (writes_in_flight > 0), and only then.
    assert!(!Engine::poller_yields(0));
    assert!(Engine::poller_yields(1));
    assert!(Engine::poller_yields(u64::MAX));
}

/// Remembered desk addresses survive a restart: `desks.json` under
/// `data_dir` reloads into `desk_addrs` (scan re-verify + /24 sweep
/// need it, broadcast never crosses subnets). Missing/no data dir and
/// a corrupt file all degrade to empty, never to a boot failure.
#[test]
fn desks_persist_across_restart() {
    let dir = std::env::temp_dir().join(format!(
        "openfader-desks-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    {
        let engine = Engine::new(EngineConfig {
            data_dir: Some(dir.clone()),
            ..EngineConfig::default()
        });
        assert!(engine.desk_addrs.read().unwrap().is_empty());
        engine.remember_desk("xair", "192.168.1.20");
        assert!(dir.join("desks.json").exists(), "desk file written");
    }
    let engine = Engine::new(EngineConfig {
        data_dir: Some(dir.clone()),
        ..EngineConfig::default()
    });
    assert_eq!(
        engine
            .desk_addrs
            .read()
            .unwrap()
            .get("xair")
            .map(String::as_str),
        Some("192.168.1.20"),
        "restart reloads the remembered address"
    );
    // No data dir: in-memory only, never panics.
    let mem = Engine::new(EngineConfig::default());
    assert!(mem.desk_addrs.read().unwrap().is_empty());
    // Corrupt file: empty map, engine still boots.
    std::fs::write(dir.join("desks.json"), "{ not json").unwrap();
    let broken = Engine::new(EngineConfig {
        data_dir: Some(dir.clone()),
        ..EngineConfig::default()
    });
    assert!(broken.desk_addrs.read().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_store_diffs() {
    let mut cache: HashMap<u16, HashMap<String, Value>> = HashMap::new();
    // New value: changed.
    assert!(cache_store(&mut cache, 0, "gain", json!(0.0)));
    // Same value: quiet (no frame when unchanged).
    assert!(!cache_store(&mut cache, 0, "gain", json!(0.0)));
    // Changed value: one frame on change.
    assert!(cache_store(&mut cache, 0, "gain", json!(1.5)));
    assert!(!cache_store(&mut cache, 0, "gain", json!(1.5)));
    // Other channels/ids are independent.
    assert!(cache_store(&mut cache, 1, "gain", json!(1.5)));
    assert!(cache_store(&mut cache, 0, "pan", json!(0.0)));
}

#[test]
fn unsupported_error_shapes() {
    let timeout = anyhow::anyhow!("/ch/01/eq/on: timed out after 2s");
    assert!(is_unsupported_error(&timeout));
    assert!(is_unsupported_error(&anyhow::anyhow!(
        "xair: unknown param \"nope\""
    )));
    assert!(is_unsupported_error(&anyhow::anyhow!(
        "xair: get_fader not supported"
    )));
    // No-data errors keep polling: empty sim slots, unpushed dl32s
    // cache, and a driver that is not connected yet.
    assert!(!is_unsupported_error(&anyhow::anyhow!(
        "sim: gain has no value yet"
    )));
    assert!(!is_unsupported_error(&anyhow::anyhow!(
        "fader: no value pushed yet"
    )));
    assert!(!is_unsupported_error(&anyhow::anyhow!(
        "not connected: call connect() first"
    )));
}

#[test]
fn new_param_id_shapes_need_no_allowlist() {
    // New CATALOGUE ids (lr_assign, solo_*, sendN_grpon, automix_weight,
    // fx_insert, snap/scope, routing src) flow through the generic strip
    // param path (`Cmd::Param` passes `field` straight to the driver
    // no match-list gates it). These pure helpers must already classify
    // the new id shapes correctly so zero engine changes are needed:
    // `sendN_grpon` rides the send slot-number rule into the bus/FX
    // send recall groups; desk-global ids fall back to `config` (never
    // silently dropped from a partial recall).
    assert_eq!(send_slot_number("send1_grpon"), Some(1));
    assert_eq!(send_slot_number("send10_grpon"), Some(10));
    assert_eq!(send_slot_number("aux3_grpon"), Some(3));
    assert_eq!(send_slot_number("fx2_grpon"), Some(2));
    assert_eq!(send_slot_number("lr_assign"), None);
    assert_eq!(param_scope_group("send1_grpon"), "sends_bus");
    assert_eq!(param_scope_group("send1_tap"), "sends_bus");
    assert_eq!(param_scope_group("send7_grpon"), "sends_fx");
    assert_eq!(param_scope_group("send10_grpon"), "sends_fx");
    assert_eq!(param_scope_group("lr_assign"), "config");
    assert_eq!(param_scope_group("automix_weight"), "config");
    assert!(is_priority_id("send1_grpon"));
    assert!(!is_priority_id("lr_assign"));
    assert!(!is_priority_id("automix_weight"));
    assert_eq!(safe_order_key("send1_grpon").0, 7);
    assert_eq!(safe_order_key("lr_assign").0, 9);
    // Channel inserts (rig-verified 2026-10-20) need no allowlist either:
    // they fall back to the config recall group, drain on the background
    // lane (never the essentials/focus fast path, first paint stays
    // faders/names), and sort last in the safe recall order.
    assert_eq!(param_scope_group("insert_on"), "config");
    assert_eq!(param_scope_group("insert_sel"), "config");
    assert!(!is_priority_id("insert_on"));
    assert!(!is_priority_id("insert_sel"));
    assert!(!is_essentials_id("insert_on"));
    assert!(!is_essentials_id("insert_sel"));
    assert_eq!(safe_order_key("insert_on").0, 9);
    assert_eq!(safe_order_key("insert_sel").0, 9);
}

#[test]
fn numbered_type_names_resolve_like_caps() {
    // `FxSetType` on a desk-native rack validates against the same
    // table `caps.fx` advertises (short code `id` or full display
    // `label`), not the design list: design names are unknown there.
    let xair = openfader_proto_xair::XAirDriver::new();
    let idx = |name: &str| crate::drivers::DynDriver::fx_type_index_of(&xair, name);
    assert_eq!(idx("HALL"), Some(0));
    assert_eq!(idx("VRM"), Some(7));
    assert_eq!(idx("PIT"), Some(60));
    assert_eq!(idx("Hall Reverb"), Some(0));
    assert_eq!(idx("Vintage Room"), Some(7));
    assert_eq!(idx("Stereo Pitch Shifter"), Some(60));
    assert_eq!(idx("Nope"), None);
    assert_eq!(idx("Hall"), None);
    assert_eq!(idx(""), None);
}

#[test]
fn probe_cmd_parses() {
    let cmd: Cmd = serde_json::from_value(json!({"cmd": "probe", "addr": "192.168.1.20"})).unwrap();
    assert!(matches!(cmd, Cmd::Probe { .. }));
    assert!(Cmd::known_tags().contains(&"probe"));
}

#[test]
fn device_via_omitted_when_absent() {
    // Additive wire change: rows without a source serialize exactly
    // like before (golden devices.json keeps passing unmodified).
    let d = DeviceDesc {
        id: "x".to_string(),
        name: "X".to_string(),
        model: "M".to_string(),
        ip: "1.2.3.4".to_string(),
        kind: "console".to_string(),
        state: "available".to_string(),
        fw: None,
        via: None,
    };
    let v = serde_json::to_value(&d).unwrap();
    assert!(v.get("via").is_none());
    assert!(v.get("fw").is_none());
}

#[tokio::test]
async fn probe_rejects_unresolvable_addr() {
    let engine = Engine::new(EngineConfig::default());
    // Garbage never resolves: silence, never a blind row, and the
    // stored device list is untouched.
    let before = engine.devices.read().unwrap().len();
    assert!(engine.probe_desk("xair", "not-a-desk!!!").await.is_none());
    assert_eq!(engine.devices.read().unwrap().len(), before);
}

#[tokio::test]
async fn probe_silent_loopback_answers_nothing() {
    let engine = Engine::new(EngineConfig::default());
    // Nothing listens on UDP 10024 here: no reply, no row.
    assert!(engine.probe_desk("xair", "127.0.0.1").await.is_none());
}
