use super::*;

#[tokio::test]
async fn matrix_and_main_sends_roundtrip_on_native_paths_with_their_own_domains() {
    let desk = Desk::new().await;
    {
        let mut state = desk.values.lock().unwrap();
        state.extend([
            ("/ch/1/send/MX8/lvl".into(), O::Float(-120.)),
            ("/ch/1/send/MX8/mode".into(), O::String("PRE".into())),
            ("/ch/1/send/MX8/pan".into(), O::Float(-25.)),
            ("/ch/1/main/4/on".into(), O::Int(0)),
            ("/ch/1/main/4/pre".into(), O::Int(0)),
            ("/aux/8/main/4/lvl".into(), O::Float(-6.)),
            ("/bus/16/send/MX1/pre".into(), O::Int(0)),
            ("/bus/16/send/2/pre".into(), O::Int(1)),
            ("/main/4/send/MX8/lvl".into(), O::Float(-9.)),
            ("/main/4/send/MX8/pre".into(), O::Int(1)),
            ("/ch/1/ptap".into(), O::String("PFL".into())),
        ]);
    }
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    for (strip, id, initial, value, path, wire) in [
        (
            0,
            "send24_level",
            ParamValue::Float(-120.),
            ParamValue::Float(-144.),
            "/ch/1/send/MX8/lvl",
            O::Float(-144.),
        ),
        (
            0,
            "send24_tap",
            ParamValue::Float(3.),
            ParamValue::Float(5.),
            "/ch/1/send/MX8/mode",
            O::String("GRP".into()),
        ),
        (
            0,
            "send24_pan",
            ParamValue::Float(-25.),
            ParamValue::Float(75.),
            "/ch/1/send/MX8/pan",
            O::Float(75.),
        ),
        (
            0,
            "send28_on",
            ParamValue::Bool(false),
            ParamValue::Bool(true),
            "/ch/1/main/4/on",
            O::Int(1),
        ),
        (
            0,
            "send28_pre",
            ParamValue::Bool(false),
            ParamValue::Bool(true),
            "/ch/1/main/4/pre",
            O::Int(1),
        ),
        (
            47,
            "send28_level",
            ParamValue::Float(-6.),
            ParamValue::Float(-3.),
            "/aux/8/main/4/lvl",
            O::Float(-3.),
        ),
        (
            63,
            "send17_pre",
            ParamValue::Bool(false),
            ParamValue::Bool(true),
            "/bus/16/send/MX1/pre",
            O::Int(1),
        ),
        (
            63,
            "send2_pre",
            ParamValue::Bool(true),
            ParamValue::Bool(false),
            "/bus/16/send/2/pre",
            O::Int(0),
        ),
        (
            67,
            "send24_level",
            ParamValue::Float(-9.),
            ParamValue::Float(0.),
            "/main/4/send/MX8/lvl",
            O::Float(0.),
        ),
        (
            67,
            "send24_pre",
            ParamValue::Bool(true),
            ParamValue::Bool(false),
            "/main/4/send/MX8/pre",
            O::Int(0),
        ),
        (
            0,
            "wing:send_tap",
            ParamValue::Float(5.),
            ParamValue::Float(1.),
            "/ch/1/ptap",
            O::String("FILT".into()),
        ),
    ] {
        let strip = StripId(strip);
        assert_eq!(
            driver.get_param(strip, id).await.unwrap(),
            initial,
            "{path}"
        );
        driver.set_param(strip, id, value.clone()).await.unwrap();
        assert_eq!(driver.get_param(strip, id).await.unwrap(), value, "{path}");
        assert_eq!(
            desk.writes.lock().unwrap().last().unwrap(),
            &(path.into(), wire)
        );
    }
    let before = desk.writes.lock().unwrap().len();
    for (strip, id, value) in [
        (0, "send29_level", ParamValue::Float(0.)),
        (0, "send17_pre", ParamValue::Bool(true)),
        (67, "send25_on", ParamValue::Bool(true)),
        (68, "send17_on", ParamValue::Bool(true)),
        (63, "send16_on", ParamValue::Bool(true)),
        (63, "send17_tap", ParamValue::Float(5.)),
        (63, "send17_pan", ParamValue::Float(0.)),
        (0, "send28_level", ParamValue::Float(f32::NAN)),
    ] {
        assert!(
            driver.set_param(StripId(strip), id, value).await.is_err(),
            "{strip} {id}"
        );
    }
    assert_eq!(desk.writes.lock().unwrap().len(), before);
}

#[test]
fn additional_send_metadata_covers_sources_without_inventing_main_to_main_sends() {
    use openfader_core::{
        params::{supports_kind, ParamRegistry},
        StripKind,
    };
    let driver = WingDriver::new();
    let mut registry = ParamRegistry::canonical();
    registry.register(driver.param_defs());
    for (id, scopes) in WingDriver::param_scopes() {
        registry.restrict_scopes(id, scopes);
    }
    assert!(supports_kind(
        registry.find("send17_level").unwrap(),
        StripKind::Main
    ));
    assert!(!supports_kind(
        registry.find("send25_level").unwrap(),
        StripKind::Main
    ));
    assert!(!supports_kind(
        registry.find("send17_pan").unwrap(),
        StripKind::Bus
    ));
    assert!(supports_kind(
        registry.find("send17_pre").unwrap(),
        StripKind::Bus
    ));
    assert!(!supports_kind(
        registry.find("send17_pre").unwrap(),
        StripKind::In
    ));
    assert!(supports_kind(
        registry.find("send28_pre").unwrap(),
        StripKind::In
    ));
    assert_eq!(registry.find("send28_level").unwrap().min, -144.);
}
