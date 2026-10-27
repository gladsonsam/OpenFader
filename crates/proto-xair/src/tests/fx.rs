use super::*;

/// FX slots (`/fx/N/par/MM`, baseline §6 row 5): bit-exact loopback
/// through the core `FxRack` hooks — set/get plus local validation.
/// Enum pars reply as INT on hardware, so the loopback seeds INT wire
/// values (multi-option indices >1 fail the old raw float range check).
#[tokio::test]
async fn driver_fx_param_roundtrip() {
    let (driver, fake) = connected_driver().await;
    driver.set_fx_param(1, 0, 0.5).await.unwrap();
    assert_eq!(driver.get_fx_param(1, 0).await.unwrap(), 0.5);
    driver.set_fx_param(4, 63, 1.0).await.unwrap();
    assert_eq!(driver.get_fx_param(4, 63).await.unwrap(), 1.0);
    assert_eq!(driver.get_fx_param(2, 10).await.unwrap(), 0.0);
    // Enum par with >2 options (DLY Mode ST/X/M on slot 1): INT wire
    // index 2 maps onto 1.0, not a range-check failure.
    fake.fx_types.lock().unwrap().insert(1, 10);
    fake.fx
        .lock()
        .unwrap()
        .insert("/fx/1/par/03".to_string(), OscType::Int(2));
    assert_eq!(driver.get_fx_param(1, 2).await.unwrap(), 1.0);
    // 14-option Pattern (D/FL par 02 on slot 3): index 1 → 1/13.
    fake.fx_types.lock().unwrap().insert(3, 25);
    fake.fx
        .lock()
        .unwrap()
        .insert("/fx/3/par/02".to_string(), OscType::Int(1));
    let v = driver.get_fx_param(3, 1).await.unwrap();
    assert!((v - 1.0 / 13.0).abs() < 1e-6, "pattern index 1 → {v}");
    // Out-of-range enum index bails, never a fabricated float.
    fake.fx
        .lock()
        .unwrap()
        .insert("/fx/3/par/02".to_string(), OscType::Int(14));
    assert!(driver.get_fx_param(3, 1).await.is_err());
    assert!(driver.set_fx_param(0, 0, 0.5).await.is_err());
    assert!(driver.set_fx_param(5, 0, 0.5).await.is_err());
    assert!(driver.set_fx_param(1, 64, 0.5).await.is_err());
    assert!(driver.set_fx_param(1, 0, 1.5).await.is_err());
    assert!(driver.set_fx_param(1, 0, f32::NAN).await.is_err());
    assert!(driver.get_fx_param(1, 64).await.is_err());
}

/// FX slot insert loopback over UDP (`crate::fx`, rig-verified
/// 2026-10-19): `/fx/N/insert` rides int `1`/`0` (slot 4 inserted on
/// the rig, the rest bypassed). Out-of-range slots bail locally.
#[tokio::test]
async fn driver_fx_insert_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert!(!driver.get_fx_insert(1).await.unwrap());
    driver.set_fx_insert(1, true).await.unwrap();
    assert!(driver.get_fx_insert(1).await.unwrap());
    driver.set_fx_insert(1, false).await.unwrap();
    assert!(!driver.get_fx_insert(1).await.unwrap());
    assert!(driver.set_fx_insert(0, true).await.is_err());
    assert!(driver.set_fx_insert(5, true).await.is_err());
    assert!(driver.get_fx_insert(0).await.is_err());
    assert!(driver.get_fx_insert(5).await.is_err());
    // Slot-addressed, not a strip param (out of strip caps).
    assert!(!XAIR_CAPABILITIES.contains(&"fx_insert"));
    assert!(driver.get_param(StripId(0), "fx_insert").await.is_err());
}

/// FX type-select write loopback over UDP (`crate::fx`, rig-verified
/// 2026-10-19 — slot-4 sweep 60→0/7/26/59/30/56→60): `set_fx_type`
/// drives the same `/fx/N/type` leaf `get_fx_type` reads. Range
/// `0..=60` enforced locally (firmware holds 61 types, one more than
/// the app table's 0..=59); out-of-range slots/types bail locally.
#[tokio::test]
async fn driver_fx_type_write_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert_eq!(driver.get_fx_type(1).await.unwrap(), 0);
    driver.set_fx_type(4, 60).await.unwrap();
    assert_eq!(driver.get_fx_type(4).await.unwrap(), 60);
    for t in [0u8, 7, 26, 30, 56, 59, 60] {
        driver.set_fx_type(4, t).await.unwrap();
        assert_eq!(driver.get_fx_type(4).await.unwrap(), t, "type {t}");
    }
    assert!(driver.set_fx_type(1, 61).await.is_err());
    assert!(driver.set_fx_type(0, 7).await.is_err());
    assert!(driver.set_fx_type(5, 7).await.is_err());
    assert!(driver.get_fx_type(0).await.is_err());
    assert!(driver.get_fx_type(5).await.is_err());
    driver.set_fx_param(4, 0, 0.25).await.unwrap();
    assert_eq!(driver.get_fx_param(4, 0).await.unwrap(), 0.25);
}
