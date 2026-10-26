use super::*;

fn source_mode_definition(readonly: bool) -> Vec<u8> {
    let mut body = vec![0; 10];
    body.extend_from_slice(&[4, b'm', b'o', b'd', b'e', 4, b'M', b'O', b'D', b'E']);
    body.extend_from_slice(&(0x50u16 | if readonly { 0x200 } else { 0 }).to_be_bytes());
    body.extend_from_slice(&3u16.to_be_bytes());
    for mode in ["M", "ST", "M/S"] {
        body.push(mode.len() as u8);
        body.extend_from_slice(mode.as_bytes());
        body.push(0);
    }
    let mut out = vec![0xdf];
    out.extend_from_slice(&(body.len() as u16).to_be_bytes());
    out.extend(body);
    out.push(0xde);
    out
}

fn configure_source_mode(desk: &Desk, group: &str, index: u8, mode: &str, readonly: bool) {
    let base = format!("/io/in/{group}/{index}");
    desk.values
        .lock()
        .unwrap()
        .insert(format!("{base}/mode"), O::String(mode.into()));
    desk.definitions
        .lock()
        .unwrap()
        .insert(base, source_mode_definition(readonly));
}

#[tokio::test]
async fn shared_source_modes_use_pinned_main_and_alternate_ports_and_live_readback() {
    use openfader_proto_wing::source;
    let desk = Desk::new().await;
    configure_source_mode(&desk, "LCL", 3, "M", false);
    configure_source_mode(&desk, "USB", 12, "M/S", false);
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    assert_eq!(
        driver.get_param(strip, source::MODE).await.unwrap(),
        ParamValue::Text("LCL/3:M".into())
    );
    assert_eq!(
        driver.get_param(strip, source::WRITABLE).await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(strip, source::MODE, ParamValue::Text("LCL/3:M>ST".into()))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(strip, source::MODE).await.unwrap(),
        ParamValue::Text("LCL/3:ST".into())
    );
    assert_eq!(driver.source_mode_capture(strip).await.unwrap(), "LCL/3:ST");
    assert!(desk
        .writes
        .lock()
        .unwrap()
        .contains(&("/io/in/LCL/3/mode".into(), O::String("ST".into()))));
    driver
        .set_param(strip, "source_select", ParamValue::Float(1.))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(strip, source::MODE).await.unwrap(),
        ParamValue::Text("USB/12:M/S".into())
    );
    let count = desk.writes.lock().unwrap().len();
    assert!(driver.set_source_mode(strip, "LCL/3:ST>M").await.is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), count);
    driver.set_source_mode(strip, "USB/12:M/S>M").await.unwrap();
    assert_eq!(
        driver.get_param(strip, source::MODE).await.unwrap(),
        ParamValue::Text("USB/12:M".into())
    );
    assert!(desk
        .writes
        .lock()
        .unwrap()
        .contains(&("/io/in/USB/12/mode".into(), O::String("M".into()))));
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn source_mode_edits_reject_stale_ports_modes_ignored_writes_and_live_readonly_flags() {
    use openfader_proto_wing::source;
    let desk = Desk::new().await;
    configure_source_mode(&desk, "LCL", 3, "M", false);
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    desk.values
        .lock()
        .unwrap()
        .insert("/io/in/LCL/3/mode".into(), O::String("M/S".into()));
    assert!(driver.set_source_mode(strip, "LCL/3:M>ST").await.is_err());
    assert!(desk.writes.lock().unwrap().is_empty());
    desk.values
        .lock()
        .unwrap()
        .insert("/io/in/LCL/3/mode".into(), O::String("M".into()));
    *desk.reroute_on_definition.lock().unwrap() = Some(("/ch/1/in/conn/in".into(), O::Int(4)));
    assert!(driver.set_source_mode(strip, "LCL/3:M>ST").await.is_err());
    assert!(desk.writes.lock().unwrap().is_empty());
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/in/conn/in".into(), O::Int(3));
    configure_source_mode(&desk, "LCL", 3, "M", true);
    assert_eq!(
        driver.get_param(strip, source::WRITABLE).await.unwrap(),
        ParamValue::Bool(false)
    );
    assert!(driver.source_mode_capture(strip).await.is_err());
    assert!(driver.set_source_mode(strip, "LCL/3:M>ST").await.is_err());
    assert!(desk.writes.lock().unwrap().is_empty());
    configure_source_mode(&desk, "LCL", 3, "M", false);
    desk.ignored_writes
        .lock()
        .unwrap()
        .insert("/io/in/LCL/3/mode".into());
    assert!(driver.set_source_mode(strip, "LCL/3:M>ST").await.is_err());
    assert_eq!(
        driver.get_param(strip, source::MODE).await.unwrap(),
        ParamValue::Text("LCL/3:M".into())
    );
    desk.ignored_writes.lock().unwrap().clear();
    driver.set_source_mode(strip, "LCL/3:M>ST").await.unwrap();
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn source_modes_cover_external_groups_and_readonly_oscillator_without_writing_internal_buses()
{
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    for group in [
        "LCL", "AUX", "A", "B", "C", "SC", "USB", "CRD", "MOD", "PLAY", "AES", "USR", "OSC",
    ] {
        configure_source_mode(&desk, group, 1, "M", group == "OSC");
        desk.values
            .lock()
            .unwrap()
            .insert("/ch/1/in/conn/grp".into(), O::String(group.into()));
        desk.values
            .lock()
            .unwrap()
            .insert("/ch/1/in/conn/in".into(), O::Int(1));
        assert_eq!(
            driver.source_mode_value(StripId(0)).await.unwrap(),
            format!("{group}/1:M")
        );
        assert_eq!(
            driver.source_mode_writable(StripId(0)).await.unwrap(),
            group != "OSC"
        );
        if group != "OSC" {
            driver
                .set_source_mode(StripId(0), &format!("{group}/1:M>ST"))
                .await
                .unwrap();
        } else {
            assert!(driver
                .set_source_mode(StripId(0), "OSC/1:M>ST")
                .await
                .is_err());
        }
    }
    let count = desk.writes.lock().unwrap().len();
    for group in ["OFF", "BUS", "MAIN", "MTX"] {
        desk.values
            .lock()
            .unwrap()
            .insert("/ch/1/in/conn/grp".into(), O::String(group.into()));
        assert!(driver.source_mode_value(StripId(0)).await.is_err());
    }
    for bad in [
        "M",
        "LCL/3:ST>bogus",
        "LCL/0:M",
        "LCL/25:ST",
        "bogus/1:M",
        "LCL/3:mono",
    ] {
        assert!(driver.set_source_mode(StripId(0), bad).await.is_err());
    }
    assert_eq!(desk.writes.lock().unwrap().len(), count);
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn input_balance_auto_selection_effective_mode_and_audio_width_use_distinct_native_domains() {
    use openfader_proto_wing::source;
    let desk = Desk::new().await;
    for root in ["/ch/1", "/aux/1"] {
        desk.values
            .lock()
            .unwrap()
            .insert(format!("{root}/in/set/$mode"), O::String("M/S".into()));
    }
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    for (strip, root) in [(0, "/ch/1"), (40, "/aux/1")] {
        assert_eq!(
            driver
                .get_param(StripId(strip), source::EFFECTIVE)
                .await
                .unwrap(),
            ParamValue::Float(2.)
        );
        for (param, value, path, arg) in [
            (
                source::BALANCE,
                ParamValue::Float(-9.),
                "in/set/bal",
                O::Float(-9.),
            ),
            (
                source::AUTO,
                ParamValue::Bool(true),
                "in/set/srcauto",
                O::Int(1),
            ),
        ] {
            driver
                .set_param(StripId(strip), param, value.clone())
                .await
                .unwrap();
            assert_eq!(
                driver.get_param(StripId(strip), param).await.unwrap(),
                value
            );
            assert!(desk
                .writes
                .lock()
                .unwrap()
                .contains(&(format!("{root}/{path}"), arg)));
        }
    }
    for (strip, root) in [
        (0, "/ch/1"),
        (40, "/aux/1"),
        (48, "/bus/1"),
        (64, "/main/1"),
        (68, "/mtx/1"),
    ] {
        driver
            .set_param(StripId(strip), source::WIDTH, ParamValue::Float(-150.))
            .await
            .unwrap();
        assert_eq!(
            driver
                .get_param(StripId(strip), source::WIDTH)
                .await
                .unwrap(),
            ParamValue::Float(-150.)
        );
        assert!(desk
            .writes
            .lock()
            .unwrap()
            .contains(&(format!("{root}/wid"), O::Float(-150.))));
    }
    let count = desk.writes.lock().unwrap().len();
    assert!(driver
        .set_param(StripId(0), source::EFFECTIVE, ParamValue::Float(1.))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), source::WRITABLE, ParamValue::Bool(true))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), source::MODE, ParamValue::Float(1.))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), source::BALANCE, ParamValue::Float(10.))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), source::WIDTH, ParamValue::Float(151.))
        .await
        .is_err());
    for strip in [48, 64, 68, 76] {
        assert!(mapping::mapping(StripId(strip), source::BALANCE).is_err());
        assert!(mapping::mapping(StripId(strip), source::AUTO).is_err());
    }
    assert!(mapping::mapping(StripId(76), source::WIDTH).is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), count);
    driver.disconnect().await.unwrap();
}

#[test]
fn source_mode_readback_keeps_port_identity_and_desired_mode_separate_from_the_old_mode_guard() {
    use openfader_proto_wing::source::tagged_agree;
    assert!(tagged_agree("LCL/3:ST", "LCL/3:M>ST"));
    assert!(tagged_agree("USB/12:M/S", "USB/12:M/S"));
    for (read, written) in [
        ("LCL/3:M", "LCL/3:M>ST"),
        ("LCL/3:ST", "LCL/4:M>ST"),
        ("LCL/3:ST", "A/3:M>ST"),
        ("LCL/3:ST", "LCL/3:invalid>ST"),
        ("X/3:M", "X/3:M"),
    ] {
        assert!(!tagged_agree(read, written));
    }
}

#[tokio::test]
async fn automatic_source_switching_requires_explicit_preamp_target_context() {
    let desk = Desk::new().await;
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/in/set/srcauto".into(), O::Int(1));
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    assert!(driver
        .set_param(StripId(0), "gain", ParamValue::Float(20.))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "phantom", ParamValue::Bool(true))
        .await
        .is_err());
    assert!(desk.writes.lock().unwrap().is_empty());
    let unrelated = BTreeMap::from([("mute".into(), ParamValue::Bool(false))]);
    assert!(driver
        .set_guarded(StripId(0), "gain", ParamValue::Float(20.), &unrelated)
        .await
        .is_err());
    let expected = BTreeMap::from([
        ("source_a".into(), ParamValue::Float(1003.)),
        ("source_b".into(), ParamValue::Float(7012.)),
        ("source_select".into(), ParamValue::Float(0.)),
    ]);
    driver
        .set_guarded(StripId(0), "gain", ParamValue::Float(20.), &expected)
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "gain").await.unwrap(),
        ParamValue::Float(20.)
    );
    assert!(desk
        .writes
        .lock()
        .unwrap()
        .contains(&("/io/in/LCL/3/g".into(), O::Float(20.))));
    driver.disconnect().await.unwrap();
}
