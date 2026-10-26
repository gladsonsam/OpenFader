use super::*;

#[test]
fn dca_tags_preserve_unrelated_tags_unicode_and_neighboring_group_numbers() {
    use openfader_proto_wing::groups::{assigned, set_assignment};
    let tags = "Vócal,#D1,#D16,#Drums, stage-left ";
    assert!(assigned(tags, 1).unwrap());
    assert!(assigned(tags, 16).unwrap());
    assert!(!assigned(tags, 6).unwrap());
    assert_eq!(
        set_assignment(tags, 1, false).unwrap(),
        "Vócal,#D16,#Drums, stage-left "
    );
    assert_eq!(
        set_assignment(tags, 16, false).unwrap(),
        "Vócal,#D1,#Drums, stage-left "
    );
    assert_eq!(set_assignment(tags, 16, true).unwrap(), tags);
    assert_eq!(
        set_assignment("TALKA.ON", 16, true).unwrap(),
        "TALKA.ON,#D16"
    );
    assert_eq!(
        set_assignment("TALKA.ON, ", 16, true).unwrap(),
        "TALKA.ON, #D16"
    );
    assert!(!assigned("free #D1,#D16-extra,#D01", 1).unwrap());
    assert_eq!(
        set_assignment("free #D1,#D16-extra", 1, true).unwrap(),
        "free #D1,#D16-extra,#D1"
    );
    assert!(set_assignment(&"x".repeat(80), 16, true).is_err());
    assert!(assigned(tags, 17).is_err());
}

#[tokio::test]
async fn dca_writes_preserve_other_tags_and_serialize_multiple_memberships() {
    use openfader_proto_wing::groups::assigned;
    let desk = Desk::new().await;
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/tags".into(), O::String("TALKA.ON, Vócal ".into()));
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let (one, sixteen) = tokio::join!(
        driver.set_param(StripId(0), "dca1_assign", ParamValue::Bool(true)),
        driver.set_param(StripId(0), "dca16_assign", ParamValue::Bool(true)),
    );
    one.unwrap();
    sixteen.unwrap();
    let tags = match desk.values.lock().unwrap().get("/ch/1/tags").unwrap() {
        O::String(tags) => tags.clone(),
        _ => panic!(),
    };
    assert!(tags.starts_with("TALKA.ON, Vócal ,"));
    assert!(assigned(&tags, 1).unwrap());
    assert!(assigned(&tags, 16).unwrap());
    driver
        .set_param(StripId(0), "dca1_assign", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "dca16_assign").await.unwrap(),
        ParamValue::Bool(true)
    );
    let tags = match desk.values.lock().unwrap().get("/ch/1/tags").unwrap() {
        O::String(tags) => tags.clone(),
        _ => panic!(),
    };
    assert_eq!(tags, "TALKA.ON, Vócal ,#D16");
}

#[test]
fn mute_group_tags_do_not_overlap_dca_or_user_tokens_and_respect_native_limits() {
    use openfader_proto_wing::groups::{
        edit, member,
        GroupKind::{Dca, Mute},
    };
    let tags = "TALKA.ON,#D8,#M1, Vócal ,#M8-extra, free #M8";
    assert!(!member(tags, Mute, 8).unwrap());
    assert!(member(tags, Dca, 8).unwrap());
    let added = edit(tags, Mute, 8, true).unwrap();
    assert_eq!(added, format!("{tags},#M8"));
    assert_eq!(edit(&added, Mute, 8, false).unwrap(), tags);
    assert!(member(&edit(tags, Mute, 1, false).unwrap(), Dca, 8).unwrap());
    assert!(!member("#M16,#M08,#M8-extra", Mute, 8).unwrap());
    assert!(member(tags, Mute, 0).is_err());
    assert!(edit(tags, Mute, 9, true).is_err());
    assert_eq!(edit(&"x".repeat(76), Mute, 8, true).unwrap().len(), 80);
    assert!(edit(&"x".repeat(77), Mute, 8, true).is_err());
}

#[tokio::test]
async fn mute_group_membership_roundtrips_for_every_audio_family() {
    let desk = Desk::new().await;
    let roots = [
        (0, "/ch/1"),
        (40, "/aux/1"),
        (48, "/bus/1"),
        (64, "/main/1"),
        (68, "/mtx/1"),
    ];
    for (_, root) in roots {
        desk.values.lock().unwrap().insert(
            format!("{root}/tags"),
            O::String("TALKA.ON,#D8,free #M8".into()),
        );
    }
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    for (strip, root) in roots {
        assert!(!driver
            .get_mute_group_assign(StripId(strip), 7)
            .await
            .unwrap());
        driver
            .set_mute_group_assign(StripId(strip), 7, true)
            .await
            .unwrap();
        assert!(driver
            .get_mute_group_assign(StripId(strip), 7)
            .await
            .unwrap());
        assert_eq!(
            driver.get_param(StripId(strip), "_mg8").await.unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            desk.writes.lock().unwrap().last().unwrap(),
            &(
                format!("{root}/tags"),
                O::String("TALKA.ON,#D8,free #M8,#M8".into())
            )
        );
        driver
            .set_mute_group_assign(StripId(strip), 7, false)
            .await
            .unwrap();
        assert_eq!(
            driver
                .get_param(StripId(strip), "dca8_assign")
                .await
                .unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            desk.values.lock().unwrap()[&format!("{root}/tags")],
            O::String("TALKA.ON,#D8,free #M8".into())
        );
    }
    let before = desk.writes.lock().unwrap().len();
    assert!(driver
        .set_mute_group_assign(StripId(0), 8, true)
        .await
        .is_err());
    assert!(driver.get_mute_group_assign(StripId(0), 255).await.is_err());
    assert!(driver
        .set_mute_group_assign(StripId(76), 0, true)
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "_mg8", ParamValue::Float(1.))
        .await
        .is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), before);
}

#[tokio::test]
async fn mixed_dca_and_mute_group_edits_serialize_without_clobbering_other_memberships() {
    use openfader_proto_wing::groups::{
        member,
        GroupKind::{Dca, Mute},
    };
    let desk = Desk::new().await;
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/tags".into(), O::String("TALKA.ON, Vócal ".into()));
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let (dca, first, eighth) = tokio::join!(
        driver.set_param(StripId(0), "dca16_assign", ParamValue::Bool(true)),
        driver.set_mute_group_assign(StripId(0), 0, true),
        driver.set_mute_group_assign(StripId(0), 7, true),
    );
    dca.unwrap();
    first.unwrap();
    eighth.unwrap();
    let tags = match desk.values.lock().unwrap().get("/ch/1/tags").unwrap() {
        O::String(tags) => tags.clone(),
        _ => panic!(),
    };
    assert!(tags.starts_with("TALKA.ON, Vócal ,"));
    assert!(member(&tags, Dca, 16).unwrap());
    assert!(member(&tags, Mute, 1).unwrap());
    assert!(member(&tags, Mute, 8).unwrap());
}

#[tokio::test]
async fn ignored_group_write_is_an_error_and_never_becomes_confirmed_membership() {
    let desk = Desk::new().await;
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/tags".into(), O::String("TALKA.ON,#D1".into()));
    desk.ignored_writes
        .lock()
        .unwrap()
        .insert("/ch/1/tags".into());
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let error = driver
        .set_mute_group_assign(StripId(0), 0, true)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("readback"));
    assert!(!driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
    assert_eq!(
        driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
        ParamValue::Bool(true)
    );
    desk.ignored_writes.lock().unwrap().clear();
    driver
        .set_mute_group_assign(StripId(0), 0, true)
        .await
        .unwrap();
    assert!(driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
}
