use super::*;

/// Assign-plane loopback over UDP (`groups` + automix container, desk
/// shapes rig-verified 2026-10-20): DCA/mute bitmask RMW, automix
/// enable container RMW (weight preserved), LR round-trip. Out-of-range
/// groups and silent roots (main/DCA) bail locally — the live desk
/// saturates out-of-range mask writes, so an unchecked index would
/// (mis)assign every group there.
#[tokio::test]
async fn driver_assign_plane_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert!(matches!(
        driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
        ParamValue::Bool(false)
    ));
    driver
        .set_param(StripId(0), "dca1_assign", ParamValue::Bool(true))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
        ParamValue::Bool(true)
    ));
    assert!(matches!(
        driver.get_param(StripId(0), "dca2_assign").await.unwrap(),
        ParamValue::Bool(false)
    ));
    driver
        .set_param(StripId(0), "dca3_assign", ParamValue::Bool(true))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
        ParamValue::Bool(true)
    ));
    driver
        .set_param(StripId(0), "dca1_assign", ParamValue::Bool(false))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
        ParamValue::Bool(false)
    ));
    assert!(matches!(
        driver.get_param(StripId(0), "dca3_assign").await.unwrap(),
        ParamValue::Bool(true)
    ));
    assert!(driver.get_param(StripId(0), "dca5_assign").await.is_err());
    assert!(driver
        .set_param(StripId(30), "dca1_assign", ParamValue::Bool(true))
        .await
        .is_err());
    assert!(matches!(
        driver.get_param(StripId(0), "lr_assign").await.unwrap(),
        ParamValue::Bool(true)
    ));
    driver
        .set_param(StripId(0), "lr_assign", ParamValue::Bool(false))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "lr_assign").await.unwrap(),
        ParamValue::Bool(false)
    ));
    driver
        .set_param(StripId(0), "lr_assign", ParamValue::Bool(true))
        .await
        .unwrap();
    assert!(!driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
    driver
        .set_mute_group_assign(StripId(0), 1, true)
        .await
        .unwrap();
    assert!(driver.get_mute_group_assign(StripId(0), 1).await.unwrap());
    assert!(!driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
    driver
        .set_mute_group_assign(StripId(0), 1, false)
        .await
        .unwrap();
    assert!(!driver.get_mute_group_assign(StripId(0), 1).await.unwrap());
    assert!(driver.get_mute_group_assign(StripId(0), 4).await.is_err());
    assert!(driver
        .set_mute_group_assign(StripId(0), 4, true)
        .await
        .is_err());
    // Main-LR (strip 30) and DCA (strip 31) strips carry no `grp`
    // leaf: local bail, never a hardware timeout.
    assert!(driver.get_mute_group_assign(StripId(30), 0).await.is_err());
    assert!(driver
        .set_mute_group_assign(StripId(31), 0, true)
        .await
        .is_err());
    driver
        .set_param(StripId(0), "automix_weight", ParamValue::Float(0.75))
        .await
        .unwrap();
    driver
        .set_param(StripId(0), "automix_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "automix_on").await.unwrap(),
        ParamValue::Bool(true)
    ));
    assert!(matches!(
        driver.get_param(StripId(0), "automix_weight").await.unwrap(),
        ParamValue::Float(w) if (w - 0.75).abs() < 1e-6
    ));
    driver
        .set_param(StripId(0), "automix_on", ParamValue::Bool(false))
        .await
        .unwrap();
    assert!(matches!(
        driver.get_param(StripId(0), "automix_on").await.unwrap(),
        ParamValue::Bool(false)
    ));
    assert!(matches!(
        driver.get_param(StripId(0), "automix_weight").await.unwrap(),
        ParamValue::Float(w) if (w - 0.75).abs() < 1e-6
    ));
}

/// Input routing loopback over UDP (`crate::input`, owner-rig verified
/// 2026-10-18 on XR18 FW 1.20): usb-return source, USB trim, phase
/// invert, HPF on/off + corner — set/get plus local validation.
/// `inputsource`/`usbinput` stay errors (live-silent, see `crate::input`).
#[tokio::test]
async fn driver_input_routing_roundtrip() {
    let (driver, _) = connected_driver().await;
    // The raw alias must not bypass the shared source/preamp guard.
    assert!(driver
        .set_param(StripId(0), "usbreturn", ParamValue::Float(17.0))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "usbreturn", ParamValue::Float(0.0))
        .await
        .is_err());
    for bad in [18.0, -1.0, 3.5, f32::NAN, f32::INFINITY] {
        assert!(
            driver
                .set_param(StripId(0), "usbreturn", ParamValue::Float(bad))
                .await
                .is_err(),
            "usbreturn {bad} accepted"
        );
    }
    assert!(driver
        .set_param(StripId(0), "usbreturn", ParamValue::Bool(true))
        .await
        .is_err());

    // trim: dB −18..=18, linear wire (the fake stores verbatim, so
    // epsilon covers f32 conversion only).
    driver
        .set_param(StripId(0), "trim", ParamValue::Float(0.0))
        .await
        .unwrap();
    let ParamValue::Float(db) = driver.get_param(StripId(0), "trim").await.unwrap() else {
        panic!("trim read back non-float");
    };
    assert!((db - 0.0).abs() < 1e-4, "trim readback {db}");
    driver
        .set_param(StripId(0), "trim", ParamValue::Float(-18.0))
        .await
        .unwrap();
    let ParamValue::Float(db) = driver.get_param(StripId(0), "trim").await.unwrap() else {
        panic!("trim read back non-float");
    };
    assert!((db + 18.0).abs() < 1e-4, "trim readback {db}");
    assert!(driver
        .set_param(StripId(0), "trim", ParamValue::Float(18.1))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "trim", ParamValue::Bool(false))
        .await
        .is_err());

    // polarity (phase invert): plain bool, NOT inverted like /mix/on.
    driver
        .set_param(StripId(0), "polarity", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "polarity").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(StripId(0), "polarity", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "polarity").await.unwrap(),
        ParamValue::Bool(false)
    );
    assert!(driver
        .set_param(StripId(0), "polarity", ParamValue::Float(1.0))
        .await
        .is_err());

    // hpf_on + hpf_freq (log 20..400 Hz; 1 Hz epsilon for log/f32).
    driver
        .set_param(StripId(0), "hpf_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "hpf_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(StripId(0), "hpf_freq", ParamValue::Float(80.0))
        .await
        .unwrap();
    let ParamValue::Float(hz) = driver.get_param(StripId(0), "hpf_freq").await.unwrap() else {
        panic!("hpf_freq read back non-float");
    };
    assert!((hz - 80.0).abs() < 1.0, "hpf_freq readback {hz}");
    driver
        .set_param(StripId(0), "hpf_freq", ParamValue::Float(400.0))
        .await
        .unwrap();
    let ParamValue::Float(hz) = driver.get_param(StripId(0), "hpf_freq").await.unwrap() else {
        panic!("hpf_freq read back non-float");
    };
    assert!((hz - 400.0).abs() < 1.0, "hpf_freq readback {hz}");
    assert!(driver
        .set_param(StripId(0), "hpf_freq", ParamValue::Float(19.9))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "hpf_freq", ParamValue::Float(f32::NAN))
        .await
        .is_err());

    // v2: strip 16 is an FX return (input-scoped leaves like trim stay
    // out-of-scope errors there); the strip space ends at 35.
    assert!(driver
        .set_param(StripId(16), "trim", ParamValue::Float(0.0))
        .await
        .is_err());
    assert!(driver.get_param(StripId(16), "polarity").await.is_err());
    assert!(driver.get_param(StripId(0), "inputsource").await.is_err());
    assert!(driver
        .set_param(StripId(0), "inputsource", ParamValue::Float(0.0))
        .await
        .is_err());
    assert!(driver.get_param(StripId(0), "usbinput").await.is_err());
}

/// Send-level loopback over UDP (`crate::mix`, UNVERIFIED-ON-RIG):
/// `aux1..6` → slots `01..06`, `fx1..4` → slots `07..10`, dB via the
/// X-Air fader law. Send mutes stay errors (no wire leaf, T7).
#[tokio::test]
async fn driver_send_level_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver
        .set_param(StripId(0), "aux1_level", ParamValue::Float(-10.0))
        .await
        .unwrap();
    let ParamValue::Float(db) = driver.get_param(StripId(0), "aux1_level").await.unwrap() else {
        panic!("aux1_level read back non-float");
    };
    assert!((db + 10.0).abs() < 1e-4, "aux1_level {db}");

    driver
        .set_param(StripId(0), "fx1_level", ParamValue::Float(0.0))
        .await
        .unwrap();
    let ParamValue::Float(fx) = driver.get_param(StripId(0), "fx1_level").await.unwrap() else {
        panic!("fx1_level read back non-float");
    };
    assert!(fx.abs() < 1e-4, "fx1_level {fx}");

    // Floor: wire 0 reads back the −90 dB −inf convention.
    driver
        .set_param(StripId(0), "aux6_level", ParamValue::Float(-90.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "aux6_level").await.unwrap(),
        ParamValue::Float(-90.0)
    );
    assert!(driver
        .set_param(StripId(0), "aux1_level", ParamValue::Float(10.1))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "aux7_level", ParamValue::Float(0.0))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "fx5_level", ParamValue::Float(0.0))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "aux1_level", ParamValue::Bool(false))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(16), "aux1_level", ParamValue::Float(0.0))
        .await
        .is_err());
}

/// Generic-send loopback over UDP (`crate::mix`, firmware-truth): `send1..6`
/// alias the bus-send slots (same as `aux1..6`), `send7..10` the FX slots
/// (same as `fx1..4`); tap rides the six-point enum
/// (`0..=5 // IN, PREEQ, POSTEQ, PRE, POST, GRP`), pan rides floats.
/// The `sendN_on` leaf has NO desk leaf (firmware send block lists no
/// on/off; `/ch/NN/mix/MM/on` live-silent, T7) and is out of caps — the
/// fake still answers it so the retained driver arm stays loopback-tested.
#[tokio::test]
async fn driver_send_tap_pan_on_roundtrip() {
    let (driver, _) = connected_driver().await;
    let s = StripId(0);
    driver
        .set_param(s, "send1_level", ParamValue::Float(-10.0))
        .await
        .unwrap();
    let ParamValue::Float(db) = driver.get_param(s, "aux1_level").await.unwrap() else {
        panic!("aux1_level read back non-float");
    };
    assert!((db + 10.0).abs() < 1e-4, "send1->aux1 alias {db}");
    driver
        .set_param(s, "send7_level", ParamValue::Float(-12.0))
        .await
        .unwrap();
    let ParamValue::Float(fx) = driver.get_param(s, "fx1_level").await.unwrap() else {
        panic!("fx1_level read back non-float");
    };
    assert!((fx + 12.0).abs() < 1e-4, "send7->fx1 alias {fx}");

    driver
        .set_param(s, "send1_tap", ParamValue::Float(4.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "send1_tap").await.unwrap(),
        ParamValue::Float(4.0)
    );
    assert!(driver
        .set_param(s, "send1_tap", ParamValue::Float(6.0))
        .await
        .is_err());
    driver
        .set_param(s, "send1_pan", ParamValue::Float(-20.0))
        .await
        .unwrap();
    let ParamValue::Float(pan) = driver.get_param(s, "send1_pan").await.unwrap() else {
        panic!("send1_pan read back non-float");
    };
    assert!((pan + 20.0).abs() < 1e-3, "send1_pan {pan}");
    driver
        .set_param(s, "send1_on", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "send1_on").await.unwrap(),
        ParamValue::Bool(false)
    );

    // Slots 11..16 have no wire; legacy send mutes stay errors (T7).
    assert!(driver
        .set_param(s, "send11_level", ParamValue::Float(0.0))
        .await
        .is_err());
    assert!(driver
        .set_param(s, "send2_level", ParamValue::Bool(true))
        .await
        .is_err());
    assert!(driver.get_param(s, "aux1_mute").await.is_err());
    assert!(driver.get_param(s, "fx1_mute").await.is_err());
}

/// LR-assign loopback over UDP (`crate::mix`, rig-verified 2026-10-19):
/// `{root}/mix/lr` rides int `1`/`0` on channel/FX-return/bus strips;
/// main LR has no `lr` leaf (its `/lr/mix` container is `(on, fader,
/// pan)`), so main/DCA/FX-send strips stay out-of-scope errors.
#[tokio::test]
async fn driver_lr_assign_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert_eq!(
        driver.get_param(StripId(0), "lr_assign").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert_eq!(
        driver.get_param(StripId(16), "lr_assign").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert_eq!(
        driver.get_param(StripId(20), "lr_assign").await.unwrap(),
        ParamValue::Bool(false)
    );
    driver
        .set_param(StripId(0), "lr_assign", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "lr_assign").await.unwrap(),
        ParamValue::Bool(false)
    );
    driver
        .set_param(StripId(20), "lr_assign", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(20), "lr_assign").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert!(driver
        .set_param(StripId(0), "lr_assign", ParamValue::Float(1.0))
        .await
        .is_err());
    assert!(driver.get_param(StripId(30), "lr_assign").await.is_err());
    assert!(driver
        .set_param(StripId(30), "lr_assign", ParamValue::Bool(true))
        .await
        .is_err());
    assert!(driver.get_param(StripId(26), "lr_assign").await.is_err());
    assert!(driver.get_param(StripId(31), "lr_assign").await.is_err());
}

/// Send group-on loopback over UDP (`crate::mix`, rig-verified
/// 2026-10-19): `{root}/mix/MM/grpon` rides int `1`/`0` next to tap;
/// all slots read `0` on the owner rig. Slots 11+ have no wire.
#[tokio::test]
async fn driver_send_grpon_roundtrip() {
    let (driver, _) = connected_driver().await;
    let s = StripId(0);
    assert_eq!(
        driver.get_param(s, "send1_grpon").await.unwrap(),
        ParamValue::Bool(false)
    );
    driver
        .set_param(s, "send1_grpon", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "send1_grpon").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(s, "send10_grpon", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(s, "send10_grpon").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert!(driver
        .set_param(s, "send1_grpon", ParamValue::Float(1.0))
        .await
        .is_err());
    assert!(driver.get_param(s, "send11_grpon").await.is_err());
    assert!(driver
        .set_param(s, "send11_grpon", ParamValue::Bool(true))
        .await
        .is_err());
    // Sends originate from inputs only (catalogue scopes).
    assert!(driver
        .set_param(StripId(20), "send1_grpon", ParamValue::Bool(true))
        .await
        .is_err());
}

/// Automix weight loopback over UDP (`crate::automix`, rig-verified
/// 2026-10-19): `/ch/NN/automix/weight` rides the wire float verbatim
/// (`0..1`, no verified engineering curve); channels only.
#[tokio::test]
async fn driver_automix_weight_roundtrip() {
    let (driver, _) = connected_driver().await;
    let s = StripId(2);
    let ParamValue::Float(w) = driver.get_param(s, "automix_weight").await.unwrap() else {
        panic!("automix_weight read back non-float");
    };
    assert!((w - 0.5).abs() < 1e-6, "automix_weight default {w}");
    driver
        .set_param(s, "automix_weight", ParamValue::Float(0.7))
        .await
        .unwrap();
    let ParamValue::Float(back) = driver.get_param(s, "automix_weight").await.unwrap() else {
        panic!("automix_weight read back non-float");
    };
    assert!((back - 0.7).abs() < 1e-6, "automix_weight {back}");
    assert!(driver
        .set_param(s, "automix_weight", ParamValue::Float(1.1))
        .await
        .is_err());
    assert!(driver
        .set_param(s, "automix_weight", ParamValue::Bool(true))
        .await
        .is_err());
    // Buses/returns carry no automix leaf (catalogue scopes).
    assert!(driver
        .get_param(StripId(20), "automix_weight")
        .await
        .is_err());
    assert!(driver
        .get_param(StripId(16), "automix_weight")
        .await
        .is_err());
}

/// Strip insert loopback over UDP (`crate::insert`, rig-verified
/// 2026-10-20): `{root}/insert/on` rides int `1`/`0`, `{root}/insert/sel`
/// rides the raw int `0..=8` (NOT 1..4 FX slots) on channel/bus/main
/// strips. Untouched reads match the rig (bypassed, nothing selected).
#[tokio::test]
async fn driver_strip_insert_roundtrip() {
    let (driver, _) = connected_driver().await;
    for s in [StripId(0), StripId(20), StripId(30)] {
        assert_eq!(
            driver.get_param(s, "insert_on").await.unwrap(),
            ParamValue::Bool(false)
        );
        assert_eq!(
            driver.get_param(s, "insert_sel").await.unwrap(),
            ParamValue::Float(0.0)
        );
    }
    driver
        .set_param(StripId(0), "insert_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "insert_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(StripId(0), "insert_sel", ParamValue::Float(7.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "insert_sel").await.unwrap(),
        ParamValue::Float(7.0)
    );
    driver
        .set_param(StripId(30), "insert_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(30), "insert_on").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert!(driver
        .set_param(StripId(0), "insert_on", ParamValue::Float(1.0))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "insert_sel", ParamValue::Float(9.0))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "insert_sel", ParamValue::Float(1.5))
        .await
        .is_err());
    assert!(driver
        .set_param(StripId(0), "insert_sel", ParamValue::Bool(true))
        .await
        .is_err());
    // Returns, FX sends and masters carry no insert leaf (the desk is
    // silent there — catalogue scopes reject before any I/O).
    for s in [StripId(16), StripId(26), StripId(31), StripId(35)] {
        assert!(driver.get_param(s, "insert_on").await.is_err());
        assert!(driver.get_param(s, "insert_sel").await.is_err());
    }
}
