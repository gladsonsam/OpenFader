use super::*;

#[test]
fn documented_discovery_and_topology() {
    let (model, raw) = parse_info_reply(
        b"WING,10.0.0.5,Front of House,wing-rack,SERIAL,3.0.5\0",
        "WING",
    )
    .unwrap();
    assert_eq!(model, "wing-rack");
    assert!(raw.contains("Front of House"));
    for bad in [
        b"WING?".as_slice(),
        b"WING,no-ip,Name,wing,S,F",
        b"WING,10.0.0.1,N,wing,S",
    ] {
        assert!(parse_info_reply(bad, "WING").is_none());
    }
    let driver = WingDriver::new();
    assert_eq!(driver.strip_count(), 92);
    for (index, expected) in [
        (0, "/ch/1"),
        (39, "/ch/40"),
        (40, "/aux/1"),
        (47, "/aux/8"),
        (48, "/bus/1"),
        (63, "/bus/16"),
        (64, "/main/1"),
        (67, "/main/4"),
        (68, "/mtx/1"),
        (75, "/mtx/8"),
        (76, "/dca/1"),
        (91, "/dca/16"),
    ] {
        assert_eq!(mapping::root(StripId(index)).unwrap().0, expected);
    }
    assert!(mapping::root(StripId(92)).is_err());
    let topo = driver.topology();
    let keys: Vec<_> = (0..92)
        .map(|n| openfader_core::strip_key_for_index(&topo, n).unwrap())
        .collect();
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique.len(), 92);
    for (n, key) in keys.iter().enumerate() {
        assert_eq!(openfader_core::strip_index_for_key(&topo, key), Some(n));
    }
    assert_eq!(keys[67], "main/04");
}

#[test]
fn engineering_values_are_distinct_from_normalized_reply_fields() {
    let m = mapping::mapping(StripId(0), "fader").unwrap();
    let response = rosc::OscMessage {
        addr: "/ch/1/fdr".into(),
        args: vec![O::String("-2.0".into()), O::Float(0.7), O::Float(-2.)],
    };
    let ParamValue::Float(v) = m.decode(&response).unwrap() else {
        panic!()
    };
    assert!((mapping::fader_db(v).unwrap() + 2.).abs() < 0.0001);
    assert_eq!(m.encode(ParamValue::Float(0.)).unwrap(), O::Float(-144.));
    assert_eq!(m.encode(ParamValue::Float(1.)).unwrap(), O::Float(10.));
    for db in [-144., -120., -90., -60., -30., -10., 0., 10.] {
        assert!((mapping::fader_db(mapping::fader_unit(db).unwrap()).unwrap() - db).abs() < 0.0001);
    }
    assert!(m.encode(ParamValue::Float(f32::NAN)).is_err());
    assert!(m
        .decode(&rosc::OscMessage {
            addr: m.path.clone(),
            args: vec![O::Float(0.7), O::Float(-2.)]
        })
        .is_err());
}

#[tokio::test]
async fn live_readback_external_edits_and_model_guards() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    assert_eq!(driver.query_info().await.unwrap().name, "My WING");
    assert!(
        (mapping::fader_db(driver.get_fader(StripId(0)).await.unwrap()).unwrap() + 2.).abs()
            < 0.0001
    );
    driver
        .set_fader(StripId(0), mapping::fader_unit(0.).unwrap())
        .await
        .unwrap();
    assert!(driver.get_fader(StripId(0)).await.is_ok());
    let O::Float(db) = desk.values.lock().unwrap()["/ch/1/fdr"] else {
        panic!()
    };
    assert!(db.abs() < 0.0001);
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/fdr".into(), O::Float(-20.));
    assert!(
        (mapping::fader_db(driver.get_fader(StripId(0)).await.unwrap()).unwrap() + 20.).abs()
            < 0.0001
    );
    assert_eq!(
        driver.get_param(StripId(0), "eq2_gain").await.unwrap(),
        ParamValue::Float(3.)
    );
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/eq/mdl".into(), O::String("SOUL".into()));
    let before = desk.writes.lock().unwrap().len();
    assert!(driver.get_param(StripId(0), "eq2_gain").await.is_err());
    assert!(driver
        .set_param(StripId(0), "eq2_gain", ParamValue::Float(4.))
        .await
        .is_err());
    assert!(driver.set_fader(StripId(92), 0.5).await.is_err());
    assert!(driver.set_fader(StripId(0), f32::NAN).await.is_err());
    assert!(driver.set_fx_model(9, "HALL").await.is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), before);
    driver.disconnect().await.unwrap();
    assert!(driver.get_fader(StripId(0)).await.is_err());
    assert!(driver.telemetry_timestamps().unwrap().0.is_none());
}

#[tokio::test]
async fn silent_peer_times_out_and_cannot_claim_connected_state() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let driver = WingDriver::new();
    let result = tokio::time::timeout(
        Duration::from_secs(4),
        driver.connect(&socket.local_addr().unwrap().to_string()),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(driver.query_info().await.is_err());
}

#[test]
fn output_eq_maps_eight_bands_without_aliasing_high_shelf_to_middle_band() {
    assert_eq!(
        mapping::mapping(StripId(0), "eq6_gain").unwrap().path,
        "/ch/1/eq/hg"
    );
    assert_eq!(
        mapping::mapping(StripId(48), "eq6_gain").unwrap().path,
        "/bus/1/eq/5g"
    );
    assert_eq!(
        mapping::mapping(StripId(48), "eq7_gain").unwrap().path,
        "/bus/1/eq/6g"
    );
    assert_eq!(
        mapping::mapping(StripId(48), "eq8_gain").unwrap().path,
        "/bus/1/eq/hg"
    );
    assert!(mapping::mapping(StripId(0), "eq7_gain").is_err());
    assert!(mapping::mapping(StripId(48), "send1_pan").is_err());
}

#[test]
fn every_advertised_parameter_has_metadata_and_only_valid_family_scopes() {
    use openfader_core::params::{supports_kind, ParamRegistry};
    let driver = WingDriver::new();
    let mut registry = ParamRegistry::canonical();
    registry.register(driver.param_defs());
    for (id, lo, hi) in driver.param_range_overrides() {
        registry.override_range(id, *lo, *hi);
    }
    for (id, kinds) in WingDriver::param_scopes() {
        registry.restrict_scopes(id, kinds);
    }
    for &id in driver.capabilities() {
        let def = registry
            .find(id)
            .unwrap_or_else(|| panic!("missing metadata for {id}"));
        for family in driver.topology() {
            if !supports_kind(def, family.kind) {
                continue;
            }
            if matches!(
                id,
                "source" | "source_a" | "source_b" | "gain" | "phantom" | "wing:eq_bands"
            ) || id.starts_with("dca")
                || id.starts_with("_mg")
                || matches!(
                    id,
                    "wing:delay_state"
                        | "wing:delay_value"
                        | "wing:source_mode"
                        | "wing:source_mode_writable"
                )
            {
                continue;
            }
            assert!(
                mapping::mapping(StripId(family.first as u16), id).is_ok(),
                "{id} incorrectly advertised for {}",
                family.key
            );
        }
    }
}
