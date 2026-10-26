use super::*;

#[tokio::test]
async fn channel_cut_filters_roundtrip_native_slopes_and_exclude_aux_and_outputs() {
    let desk = Desk::new().await;
    for (leaf, value) in [
        ("lc", O::Int(1)),
        ("lcf", O::Float(123.)),
        ("lcs", O::String("18".into())),
        ("hc", O::Int(1)),
        ("hcf", O::Float(4500.)),
        ("hcs", O::String("12".into())),
    ] {
        desk.values
            .lock()
            .unwrap()
            .insert(format!("/ch/1/flt/{leaf}"), value);
    }
    // These filters are independent of the EQ plugin/model and EQ bypass.
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/eq/mdl".into(), O::String("SOUL".into()));
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    assert_eq!(
        driver.get_param(strip, "hpf_slope").await.unwrap(),
        ParamValue::Float(2.)
    );
    assert_eq!(
        driver.get_param(strip, "wing:hcf_freq").await.unwrap(),
        ParamValue::Float(4500.)
    );
    for (param, value, path, wire) in [
        ("hpf_on", ParamValue::Bool(false), "lc", O::Int(0)),
        ("hpf_freq", ParamValue::Float(2000.), "lcf", O::Float(2000.)),
        ("wing:hcf_on", ParamValue::Bool(false), "hc", O::Int(0)),
        (
            "wing:hcf_freq",
            ParamValue::Float(50.),
            "hcf",
            O::Float(50.),
        ),
        (
            "wing:hcf_slope",
            ParamValue::Float(6.),
            "hcs",
            O::String("6".into()),
        ),
    ] {
        driver.set_param(strip, param, value.clone()).await.unwrap();
        assert_eq!(driver.get_param(strip, param).await.unwrap(), value);
        assert!(desk
            .writes
            .lock()
            .unwrap()
            .contains(&(format!("/ch/1/flt/{path}"), wire)));
    }
    for (index, wire) in [(0., "6"), (1., "12"), (2., "18"), (3., "24")] {
        driver
            .set_param(strip, "hpf_slope", ParamValue::Float(index))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(strip, "hpf_slope").await.unwrap(),
            ParamValue::Float(index)
        );
        assert!(desk
            .writes
            .lock()
            .unwrap()
            .contains(&("/ch/1/flt/lcs".into(), O::String(wire.into()))));
    }
    let before = desk.writes.lock().unwrap().len();
    for (param, value) in [
        ("hpf_slope", ParamValue::Float(4.)),
        ("wing:hcf_slope", ParamValue::Float(18.)),
        ("wing:hcf_freq", ParamValue::Float(49.)),
        ("wing:hcf_on", ParamValue::Float(1.)),
    ] {
        assert!(driver.set_param(strip, param, value).await.is_err());
    }
    for index in [40, 47, 48, 64, 68, 76] {
        for param in [
            "hpf_on",
            "hpf_freq",
            "hpf_slope",
            "wing:hcf_on",
            "wing:hcf_freq",
            "wing:filter_model",
        ] {
            assert!(
                mapping::mapping(StripId(index), param).is_err(),
                "{index} {param}"
            );
        }
    }
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/flt/lcs".into(), O::String("36".into()));
    assert!(driver.get_param(strip, "hpf_slope").await.is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), before);
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn tool_filter_models_use_actual_wire_names_and_guard_model_specific_fields() {
    let desk = Desk::new().await;
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/flt/mdl".into(), O::String("TILT".into()));
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/flt/tf".into(), O::Int(0));
    for (leaf, value) in [
        ("tilt", 0.),
        ("low", 0.),
        ("proc", 0.),
        ("f", 1000.),
        ("q", 1.),
    ] {
        desk.values
            .lock()
            .unwrap()
            .insert(format!("/ch/1/flt/{leaf}"), O::Float(value));
    }
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    driver
        .set_param(strip, "wing:filter_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(strip, "wing:filter_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    for (model, index, param, value, leaf) in [
        ("TILT", 0., "wing:filter_tilt", -3., "tilt"),
        ("MAX", 1., "wing:filter_low", 70., "low"),
        ("MAX", 1., "wing:filter_high", 40., "proc"),
        ("AP1", 2., "wing:filter_ap1_freq", 2500., "f"),
        ("AP2", 3., "wing:filter_ap2_freq", 4000., "f"),
        ("AP2", 3., "wing:filter_ap2_q", 2.5, "q"),
    ] {
        driver
            .set_param(strip, "wing:filter_model", ParamValue::Float(index))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(strip, "wing:filter_model").await.unwrap(),
            ParamValue::Float(index)
        );
        assert_eq!(
            desk.values.lock().unwrap().get("/ch/1/flt/mdl"),
            Some(&O::String(model.into()))
        );
        driver
            .set_param(strip, param, ParamValue::Float(value))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(strip, param).await.unwrap(),
            ParamValue::Float(value)
        );
        assert!(desk
            .writes
            .lock()
            .unwrap()
            .contains(&(format!("/ch/1/flt/{leaf}"), O::Float(value))));
        let count = desk.writes.lock().unwrap().len();
        desk.values.lock().unwrap().insert(
            "/ch/1/flt/mdl".into(),
            O::String(if model == "TILT" { "MAX" } else { "TILT" }.into()),
        );
        assert!(driver.get_param(strip, param).await.is_err());
        assert!(driver
            .set_param(strip, param, ParamValue::Float(value))
            .await
            .is_err());
        assert_eq!(desk.writes.lock().unwrap().len(), count);
    }
    let count = desk.writes.lock().unwrap().len();
    assert!(driver
        .set_param(strip, "wing:filter_model", ParamValue::Float(4.))
        .await
        .is_err());
    assert!(driver
        .set_param(strip, "wing:filter_ap2_q", ParamValue::Float(f32::NAN))
        .await
        .is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), count);
    assert!(desk
        .writes
        .lock()
        .unwrap()
        .iter()
        .all(|(path, _)| !path.contains("/TILT/")
            && !path.contains("/MAX/")
            && !path.contains("/AP1/")
            && !path.contains("/AP2/")));
    driver.disconnect().await.unwrap();
}
