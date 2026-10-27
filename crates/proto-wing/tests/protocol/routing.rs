use super::*;

#[tokio::test]
async fn preamp_edits_pin_physical_source_and_reject_stale_dialogs() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "source_a").await.unwrap(),
        ParamValue::Float(1003.)
    );
    assert_eq!(
        driver.get_param(StripId(0), "gain").await.unwrap(),
        ParamValue::Float(12.)
    );
    let expected = BTreeMap::from([
        ("source_a".into(), ParamValue::Float(1003.)),
        ("source_select".into(), ParamValue::Float(0.)),
    ]);
    driver
        .set_guarded(StripId(0), "phantom", ParamValue::Bool(true), &expected)
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "phantom").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert_eq!(
        desk.writes.lock().unwrap().last().unwrap().0,
        "/io/in/LCL/3/vph"
    );
    desk.values
        .lock()
        .unwrap()
        .insert("/ch/1/in/set/altsrc".into(), O::Int(1));
    let before = desk.writes.lock().unwrap().len();
    assert!(driver
        .set_guarded(StripId(0), "phantom", ParamValue::Bool(false), &expected)
        .await
        .is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), before);
    assert_eq!(
        driver.get_param(StripId(0), "source").await.unwrap(),
        ParamValue::Text("USB 12".into())
    );
}

#[tokio::test]
async fn routing_is_one_node_update_and_is_checked_against_desk() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    assert_eq!(
        driver.patch(StripId(0), "alt", "A", 48).await.unwrap(),
        "A 48"
    );
    assert_eq!(
        desk.writes.lock().unwrap().last().unwrap(),
        &(
            "/ch/1/in/conn".into(),
            O::String("altgrp=A,altin=48".into())
        )
    );
    assert_eq!(
        driver.patch(StripId(0), "alt", "OFF", 0).await.unwrap(),
        "—"
    );
    assert!(driver.patch(StripId(0), "main", "A", 49).await.is_err());
    assert!(driver.patch(StripId(48), "main", "LCL", 1).await.is_err());
}
