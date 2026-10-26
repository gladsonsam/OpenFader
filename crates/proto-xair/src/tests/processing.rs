use super::*;

/// EQ loopback over UDP (`crate::eq`, UNVERIFIED-ON-RIG): on/off plus
/// all four bands of gain/freq/q/type, with the inverted Q wire.
#[tokio::test]
async fn driver_eq_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver
        .set_param(StripId(1), "eq_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(1), "eq_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(StripId(1), "eq_on", ParamValue::Bool(false))
        .await
        .unwrap();
    assert!(
        driver
            .set_param(StripId(1), "eq_on", ParamValue::Float(1.0))
            .await
            .is_err()
    );

    driver
        .set_param(StripId(1), "eq2_gain", ParamValue::Float(6.0))
        .await
        .unwrap();
    let ParamValue::Float(g) = driver.get_param(StripId(1), "eq2_gain").await.unwrap() else {
        panic!("eq2_gain read back non-float");
    };
    assert!((g - 6.0).abs() < 1e-4, "eq2_gain {g}");

    driver
        .set_param(StripId(1), "eq2_freq", ParamValue::Float(1000.0))
        .await
        .unwrap();
    let ParamValue::Float(f) = driver.get_param(StripId(1), "eq2_freq").await.unwrap() else {
        panic!("eq2_freq read back non-float");
    };
    // Desk log quantization is 201 steps: 1 Hz epsilon, like hpf.
    assert!((f - 1000.0).abs() < 1.0, "eq2_freq {f}");

    driver
        .set_param(StripId(1), "eq2_q", ParamValue::Float(1.4))
        .await
        .unwrap();
    let ParamValue::Float(q) = driver.get_param(StripId(1), "eq2_q").await.unwrap() else {
        panic!("eq2_q read back non-float");
    };
    assert!((q - 1.4).abs() / 1.4 < 0.06, "eq2_q {q}");

    driver
        .set_param(StripId(1), "eq2_type", ParamValue::Float(2.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(1), "eq2_type").await.unwrap(),
        ParamValue::Float(2.0)
    );

    // Band 1 carries a type on X-Air (unlike the DL32S layout).
    driver
        .set_param(StripId(1), "eq1_type", ParamValue::Float(1.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(1), "eq1_type").await.unwrap(),
        ParamValue::Float(1.0)
    );

    assert!(
        driver
            .set_param(StripId(1), "eq2_gain", ParamValue::Float(15.1))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(1), "eq2_freq", ParamValue::Float(19.9))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(1), "eq2_q", ParamValue::Float(0.29))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(1), "eq2_type", ParamValue::Float(6.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(1), "eq5_gain", ParamValue::Float(0.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(1), "eq2_gain", ParamValue::Bool(true))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(40), "eq1_gain", ParamValue::Float(0.0))
            .await
            .is_err()
    );
}

/// Gate/comp on/off loopback over UDP (`crate::dynamics`,
/// UNVERIFIED-ON-RIG): plain int `1`/`0` switches.
#[tokio::test]
async fn driver_gate_comp_on_roundtrip() {
    let (driver, _) = connected_driver().await;
    for id in ["gate_on", "comp_on"] {
        driver
            .set_param(StripId(2), id, ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(2), id).await.unwrap(),
            ParamValue::Bool(true),
            "{id}"
        );
        driver
            .set_param(StripId(2), id, ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(2), id).await.unwrap(),
            ParamValue::Bool(false),
            "{id}"
        );
        assert!(
            driver
                .set_param(StripId(2), id, ParamValue::Float(1.0))
                .await
                .is_err(),
            "{id} wrong kind accepted"
        );
    }
    assert!(
        driver
            .set_param(StripId(40), "gate_on", ParamValue::Bool(true))
            .await
            .is_err()
    );
}

/// Gate full-set loopback over UDP (`crate::gate`, firmware-truth):
/// thr/range/att ride lin wires, hold/rel ride log wires, mode/keysrc/
/// filter leaves ride ints. Live values from the owner rig (XR18 FW
/// 1.20) decode through the same codecs.
#[tokio::test]
async fn driver_gate_full_roundtrip() {
    let (driver, _) = connected_driver().await;
    let s = StripId(2);
    for (id, v) in [
        ("gate_thr", -30.0f32),
        ("gate_range", 40.0),
        ("gate_att", 5.0),
        ("gate_hold", 120.0),
        ("gate_rel", 250.0),
        ("gate_filter_freq", 1000.0),
    ] {
        driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
        let ParamValue::Float(back) = driver.get_param(s, id).await.unwrap() else {
            panic!("{id} read back non-float");
        };
        assert!(
            (back - v).abs() / v.abs().max(1.0) < 1e-2,
            "{id} {v} -> {back}"
        );
    }
    for (id, v) in [
        ("gate_mode", 4.0f32),
        ("gate_key_src", 22.0),
        ("gate_filter_type", 8.0),
    ] {
        driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
        assert_eq!(
            driver.get_param(s, id).await.unwrap(),
            ParamValue::Float(v),
            "{id}"
        );
    }
    driver
        .set_param(s, "gate_filter_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "gate_filter_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert!(
        driver
            .set_param(s, "gate_thr", ParamValue::Float(-80.1))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "gate_range", ParamValue::Float(2.9))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "gate_mode", ParamValue::Float(5.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "gate_key_src", ParamValue::Float(23.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "gate_thr", ParamValue::Bool(true))
            .await
            .is_err()
    );
    // DCA strips carry no gate block (catalogue scopes).
    assert!(
        driver
            .set_param(StripId(31), "gate_thr", ParamValue::Float(-30.0))
            .await
            .is_err()
    );
    assert!(driver.get_param(StripId(31), "gate_mode").await.is_err());
    // Channels carry 4 EQ bands (bus/main carry 6).
    assert!(driver.get_param(s, "eq5_gain").await.is_err());
    assert!(driver.get_param(StripId(20), "eq5_gain").await.is_ok());
}

/// Comp full-set loopback over UDP (`crate::comp`, firmware-truth):
/// ratio rides the INT step-table index (4.0 ↔ index 6), knee/att/makeup
/// ride lin wires, hold/rel ride log wires. Live rig values (ratio
/// `Int(5)` = 3.0 on ch01) decode through the same table.
#[tokio::test]
async fn driver_comp_full_roundtrip() {
    let (driver, _) = connected_driver().await;
    let s = StripId(2);
    for (id, v) in [
        ("comp_thr", -20.0f32),
        ("comp_ratio", 4.0),
        ("comp_knee", 3.0),
        ("comp_makeup", 6.0),
        ("comp_att", 10.0),
        ("comp_hold", 20.0),
        ("comp_rel", 100.0),
        ("comp_mix", 80.0),
        ("comp_filter_freq", 2000.0),
    ] {
        driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
        let ParamValue::Float(back) = driver.get_param(s, id).await.unwrap() else {
            panic!("{id} read back non-float");
        };
        assert!(
            (back - v).abs() / v.abs().max(1.0) < 1e-2,
            "{id} {v} -> {back}"
        );
    }
    for (id, v) in [
        ("comp_mode", 1.0f32),
        ("comp_det", 1.0),
        ("comp_env", 0.0),
        ("comp_key_src", 22.0),
        ("comp_filter_type", 8.0),
    ] {
        driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
        assert_eq!(
            driver.get_param(s, id).await.unwrap(),
            ParamValue::Float(v),
            "{id}"
        );
    }
    driver
        .set_param(s, "comp_auto", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "comp_auto").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(s, "comp_filter_on", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "comp_filter_on").await.unwrap(),
        ParamValue::Bool(false)
    );
    assert!(
        driver
            .set_param(s, "comp_ratio", ParamValue::Float(3.5))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "comp_ratio", ParamValue::Float(101.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "comp_att", ParamValue::Float(121.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(s, "comp_auto", ParamValue::Float(1.0))
            .await
            .is_err()
    );
    // FX-send masters carry no comp block (catalogue scopes).
    assert!(driver.get_param(StripId(26), "comp_thr").await.is_err());
}
