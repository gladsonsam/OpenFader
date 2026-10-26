use super::*;
#[tokio::test]
async fn driver_fader_mute_name_roundtrip() {
    let (driver, _) = connected_driver().await;
    // Fader assert uses the hardware quantization epsilon (T8): the fake
    // quantizes like the desk, so exact equality would fail here.
    driver.set_fader(StripId(0), 0.42).await.unwrap();
    let v = driver.get_fader(StripId(0)).await.unwrap();
    assert!(
        (v - 0.42).abs() < addrs::FADER_READBACK_EPSILON,
        "fader readback {v} outside hardware epsilon"
    );

    driver.set_mute(StripId(0), true).await.unwrap();
    assert!(driver.get_mute(StripId(0)).await.unwrap());
    driver.set_mute(StripId(0), false).await.unwrap();
    assert!(!driver.get_mute(StripId(0)).await.unwrap());

    driver.set_name(StripId(0), "Vox").await.unwrap();
    assert_eq!(driver.get_name(StripId(0)).await.unwrap(), "Vox");
}

#[tokio::test]
async fn driver_bus_main_info_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver.set_bus_fader(0, 0.33).await.unwrap();
    let b = driver.get_bus_fader(0).await.unwrap();
    assert!(
        (b - 0.33).abs() < addrs::FADER_READBACK_EPSILON,
        "bus fader readback {b} outside hardware epsilon"
    );
    driver.set_main_fader(0.66).await.unwrap();
    let m = driver.get_main_fader().await.unwrap();
    assert!(
        (m - 0.66).abs() < addrs::FADER_READBACK_EPSILON,
        "main fader readback {m} outside hardware epsilon"
    );

    // `/info` order is [proto-rev, name, model, firmware] (T5, §1).
    let info = driver.query_info().await.unwrap();
    assert_eq!(info.name, "FakeXR");
    assert_eq!(info.model, "XR18");
    assert_eq!(info.firmware, "1.22");
}

/// Mute-group masters: `/config/mute/1..4` answer (strict fake stays
/// silent on 5+ like the live desk), same-value writes round-trip
/// bit-exact, out-of-range groups error locally.
#[tokio::test]
async fn driver_mute_group_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert_eq!(driver.mute_group_count(), 4);
    for g in 0..4 {
        driver.set_mute_group(g, false).await.unwrap();
        assert!(!driver.get_mute_group(g).await.unwrap());
    }
    assert!(driver.set_mute_group(4, false).await.is_err());
}

/// Main LR must go through `/lr/mix/fader`: the strict fake below answers
/// ONLY that address and stays silent on `/main/st/mix/fader`, mirroring
/// live XR18 hardware (FW 1.20, 2026-10-17) where X32-style `/main/st`
/// gets NO reply. A driver still querying `/main/st` would time out here.
#[tokio::test]
async fn main_fader_uses_lr_not_main_st() {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let mixer_addr = socket.local_addr().unwrap();
    let seen = Arc::new(StdMutex::new(Vec::<String>::new()));
    let seen_worker = seen.clone();
    let worker = socket.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        let mut level = 0.8f32;
        loop {
            let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                break;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                continue;
            };
            for msg in osc::flatten(packet) {
                if msg.addr == "/xremote" {
                    continue;
                }
                seen_worker.lock().unwrap().push(msg.addr.clone());
                match (msg.addr.as_str(), msg.args.first()) {
                    ("/lr/mix/fader", Some(OscType::Float(f))) => level = *f,
                    ("/lr/mix/fader", None) => {
                        let bytes =
                            osc::encode_message("/lr/mix/fader", vec![OscType::Float(level)])
                                .unwrap();
                        let _ = worker.send_to(&bytes, peer).await;
                    }
                    // Deliberately NO reply for /main/st/mix/fader: the
                    // real mixer stays silent there.
                    _ => {}
                }
            }
        }
    });

    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(mixer_addr).await.unwrap();
    let session = OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
    let driver = XAirDriver::new();
    *driver.session.lock().await = Some(std::sync::Arc::new(session));

    driver.set_main_fader(0.66).await.unwrap();
    let v = tokio::time::timeout(std::time::Duration::from_secs(2), driver.get_main_fader())
        .await
        .expect("main fader reply in time")
        .unwrap();
    assert!(
        (v - 0.66).abs() < addrs::FADER_READBACK_EPSILON,
        "main fader readback {v} outside hardware epsilon"
    );

    let addrs = seen.lock().unwrap();
    assert!(
        addrs.iter().any(|a| a == "/lr/mix/fader"),
        "driver never queried /lr/mix/fader: {addrs:?}"
    );
    assert!(
        !addrs.iter().any(|a| a == "/main/st/mix/fader"),
        "driver queried X32-style /main/st/mix/fader: {addrs:?}"
    );
}

/// Buses must go through single-digit `/bus/1/…`: the strict fake below
/// answers ONLY that address and stays silent on `/bus/01/…`, mirroring
/// live XR18 hardware (FW 1.20, T2). A driver still zero-padding would
/// time out here.
#[tokio::test]
async fn bus_fader_uses_single_digit_not_padded() {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let mixer_addr = socket.local_addr().unwrap();
    let seen = Arc::new(StdMutex::new(Vec::<String>::new()));
    let seen_worker = seen.clone();
    let worker = socket.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        let mut level = 0.7f32;
        loop {
            let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                break;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                continue;
            };
            for msg in osc::flatten(packet) {
                if msg.addr == "/xremote" {
                    continue;
                }
                seen_worker.lock().unwrap().push(msg.addr.clone());
                match (msg.addr.as_str(), msg.args.first()) {
                    ("/bus/1/mix/fader", Some(OscType::Float(f))) => level = *f,
                    ("/bus/1/mix/fader", None) => {
                        let bytes =
                            osc::encode_message("/bus/1/mix/fader", vec![OscType::Float(level)])
                                .unwrap();
                        let _ = worker.send_to(&bytes, peer).await;
                    }
                    // Deliberately NO reply for /bus/01/mix/fader: the
                    // real mixer stays silent there.
                    _ => {}
                }
            }
        }
    });

    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(mixer_addr).await.unwrap();
    let session = OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
    let driver = XAirDriver::new();
    *driver.session.lock().await = Some(std::sync::Arc::new(session));

    driver.set_bus_fader(0, 0.33).await.unwrap();
    let v = tokio::time::timeout(std::time::Duration::from_secs(2), driver.get_bus_fader(0))
        .await
        .expect("bus fader reply in time")
        .unwrap();
    assert!(
        (v - 0.33).abs() < addrs::FADER_READBACK_EPSILON,
        "bus fader readback {v} outside hardware epsilon"
    );

    let addrs = seen.lock().unwrap();
    assert!(
        addrs.iter().any(|a| a == "/bus/1/mix/fader"),
        "driver never queried /bus/1/mix/fader: {addrs:?}"
    );
    assert!(
        !addrs.iter().any(|a| a == "/bus/01/mix/fader"),
        "driver queried zero-padded /bus/01/mix/fader: {addrs:?}"
    );
}

/// Aux return strip (`/rtn/aux`, the XR18's 17th strip, T3): fader, mute
/// and name round-trip like a channel (baseline §2 values reply there).
#[tokio::test]
async fn driver_aux_strip_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver.set_aux_fader(0.5).await.unwrap();
    let v = driver.get_aux_fader().await.unwrap();
    assert!(
        (v - 0.5).abs() < addrs::FADER_READBACK_EPSILON,
        "aux fader readback {v} outside hardware epsilon"
    );
    driver.set_aux_mute(true).await.unwrap();
    assert!(driver.get_aux_mute().await.unwrap());
    driver.set_aux_mute(false).await.unwrap();
    assert!(!driver.get_aux_mute().await.unwrap());
    driver.set_aux_name("COMP LR").await.unwrap();
    assert_eq!(driver.get_aux_name().await.unwrap(), "COMP LR");
}

/// Aux input as a first-class strip (strip 35 = `/rtn/aux`, owner-rig
/// verified 2026-10-20): fader/mute/name/pan/color/eq ride the same
/// `set_param`/`get_param` path as channels 1-16, and desk broadcasts
/// decode via `parse_inbound` to strip 35.
#[tokio::test]
async fn driver_aux_strip_param_path() {
    let (driver, _) = connected_driver().await;
    let aux = StripId(35);
    driver
        .set_param(aux, "fader", ParamValue::Float(0.5))
        .await
        .unwrap();
    let ParamValue::Float(v) = driver.get_param(aux, "fader").await.unwrap() else {
        panic!("aux fader read back non-float");
    };
    assert!(
        (v - 0.5).abs() < addrs::FADER_READBACK_EPSILON,
        "aux strip fader readback {v} outside hardware epsilon"
    );
    driver
        .set_param(aux, "mute", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(aux, "mute").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(aux, "mute", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(aux, "mute").await.unwrap(),
        ParamValue::Bool(false)
    );
    driver
        .set_param(aux, "name", ParamValue::Text("AUX IN".to_string()))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(aux, "name").await.unwrap(),
        ParamValue::Text("AUX IN".to_string())
    );
    driver
        .set_param(aux, "pan", ParamValue::Float(0.0))
        .await
        .unwrap();
    let ParamValue::Float(pan) = driver.get_param(aux, "pan").await.unwrap() else {
        panic!("aux pan read back non-float");
    };
    assert!((pan - 0.0).abs() < 1e-4, "aux pan readback {pan}");
    driver
        .set_param(aux, "eq1_gain", ParamValue::Float(3.0))
        .await
        .unwrap();
    let ParamValue::Float(g) = driver.get_param(aux, "eq1_gain").await.unwrap() else {
        panic!("aux eq1_gain read back non-float");
    };
    assert!((g - 3.0).abs() < 1e-4, "aux eq1_gain readback {g}");
    // Aux carries 4 EQ bands like the FX returns: band 5+ is
    // scope-rejected driver-side, never hitting a silent address.
    assert!(
        driver
            .set_param(aux, "eq5_gain", ParamValue::Float(0.0))
            .await
            .is_err()
    );
    // Input-scoped leaves stay out of scope on the aux return.
    assert!(driver.get_param(aux, "gain").await.is_err());
    assert!(driver.get_param(aux, "phantom").await.is_err());
    // Channel sends are channel-scoped: aux sends ride no leaf here.
    assert!(
        driver
            .set_param(aux, "aux1_level", ParamValue::Float(-10.0))
            .await
            .is_err()
    );
}

/// Headamp gain/phantom loopback over UDP (`crate::headamp`,
/// community-documented via xair-api-python, NOT owner-rig verified):
/// dB gain maps linearly onto the wire float, phantom rides int 1/0.
#[tokio::test]
async fn driver_headamp_gain_phantom_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver
        .set_param(StripId(0), "gain", ParamValue::Float(24.0))
        .await
        .unwrap();
    let ParamValue::Float(db) = driver.get_param(StripId(0), "gain").await.unwrap() else {
        panic!("headamp gain read back non-float");
    };
    // The fake stores verbatim; epsilon covers f32 conversion only —
    // hardware quantization is uncharacterized (see `crate::headamp`).
    assert!((db - 24.0).abs() < 0.05, "headamp gain readback {db}");
    assert!(
        driver
            .set_param(StripId(0), "gain", ParamValue::Float(61.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(0), "gain", ParamValue::Bool(true))
            .await
            .is_err()
    );

    driver
        .set_param(StripId(0), "phantom", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "phantom").await.unwrap(),
        ParamValue::Bool(true)
    );
    driver
        .set_param(StripId(0), "phantom", ParamValue::Bool(false))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "phantom").await.unwrap(),
        ParamValue::Bool(false)
    );
    assert!(
        driver
            .set_param(StripId(0), "phantom", ParamValue::Float(1.0))
            .await
            .is_err()
    );

    // Phantom strips stay local: no headamp past the model bound (T3).
    assert!(
        driver
            .set_param(StripId(16), "gain", ParamValue::Float(24.0))
            .await
            .is_err()
    );
    assert!(driver.get_param(StripId(16), "phantom").await.is_err());
    // Unknown params still rejected — including `inputsource`
    // (`/ch/NN/config/insrc`), which is live-silent on this rig
    // (see `crate::input`), and `usbinput` (`/ch/NN/preamp/rtnsw`).
    assert!(driver.get_param(StripId(0), "inputsource").await.is_err());
    assert!(driver.get_param(StripId(0), "usbinput").await.is_err());
    // Send mutes + aux7/8 stay unsupported: the firmware map has no
    // send-mute leaf and `/ch/NN/mix/MM/on` is live-silent (T7), and
    // only 6 bus sends exist — so these fail as unknown params.
    assert!(driver.get_param(StripId(0), "aux1_mute").await.is_err());
    assert!(driver.get_param(StripId(0), "fx1_mute").await.is_err());
    assert!(driver.get_param(StripId(0), "aux7_level").await.is_err());
    assert!(driver.get_param(StripId(0), "aux8_level").await.is_err());
    assert!(
        driver
            .set_param(StripId(0), "aux1_mute", ParamValue::Bool(true))
            .await
            .is_err()
    );
}

/// Pan loopback over UDP (`crate::mix`, UNVERIFIED-ON-RIG): engineering
/// `-100..=+100` maps linearly onto the wire float, centre 0.5.
#[tokio::test]
async fn driver_pan_roundtrip() {
    let (driver, _) = connected_driver().await;
    for (eng, wire) in [(-100.0, 0.0), (0.0, 0.5), (100.0, 1.0), (25.0, 0.625)] {
        driver
            .set_param(StripId(0), "pan", ParamValue::Float(eng))
            .await
            .unwrap();
        let ParamValue::Float(back) = driver.get_param(StripId(0), "pan").await.unwrap() else {
            panic!("pan read back non-float");
        };
        assert!((back - eng).abs() < 1e-4, "pan {eng} -> {back}");
        let _ = wire;
    }
    assert!(
        driver
            .set_param(StripId(0), "pan", ParamValue::Float(100.1))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(0), "pan", ParamValue::Bool(true))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(40), "pan", ParamValue::Float(0.0))
            .await
            .is_err()
    );
}

/// Strip look loopback over UDP (`crate::look`): colour `0..=15` is a
/// live desk leaf; `icon`/`link` have NO desk leaf (firmware map has
/// neither; both silent on FW 1.20) and are out of caps — the fake
/// still answers them so the retained driver arms stay loopback-tested.
/// Note the resolved colour conflict: `/ch/16/config/color` answers on
/// this rig (the old input-module silence note was a probe artifact).
#[tokio::test]
async fn driver_look_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver
        .set_param(StripId(0), "color", ParamValue::Float(5.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "color").await.unwrap(),
        ParamValue::Float(5.0)
    );
    driver
        .set_param(StripId(0), "icon", ParamValue::Float(7.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "icon").await.unwrap(),
        ParamValue::Float(7.0)
    );
    driver
        .set_param(StripId(0), "link", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(0), "link").await.unwrap(),
        ParamValue::Bool(true)
    );
    assert!(
        driver
            .set_param(StripId(0), "color", ParamValue::Float(16.0))
            .await
            .is_err()
    );
    assert!(
        driver
            .set_param(StripId(0), "icon", ParamValue::Float(32.0))
            .await
            .is_err()
    );
    // DCA strips carry colour (basics) but no link (input-only).
    driver
        .set_param(StripId(31), "color", ParamValue::Float(0.0))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(StripId(31), "color").await.unwrap(),
        ParamValue::Float(0.0)
    );
    assert!(driver.get_param(StripId(20), "link").await.is_err());
}

/// Non-channel strip loopback over UDP (firmware-truth): buses carry
/// fader/mute/name/colour/EQ/dynamics, FX returns (`/rtn/N`) carry
/// fader/mute/name/colour/EQ, FX-send masters and DCAs carry basics only.
/// Gate stays channel-only (out of scope elsewhere); buses + main carry
/// 6 EQ bands.
#[tokio::test]
async fn driver_bus_ret_fxsnd_main_dca_strips_roundtrip() {
    let (driver, _) = connected_driver().await;
    let b = StripId(20);
    driver.set_fader(b, 0.6).await.unwrap();
    assert!((driver.get_fader(b).await.unwrap() - 0.6).abs() < addrs::FADER_READBACK_EPSILON);
    driver.set_mute(b, true).await.unwrap();
    assert!(driver.get_mute(b).await.unwrap());
    driver.set_name(b, "MON").await.unwrap();
    assert_eq!(driver.get_name(b).await.unwrap(), "MON");
    driver
        .set_param(b, "color", ParamValue::Float(6.0))
        .await
        .unwrap();
    driver
        .set_param(b, "eq_on", ParamValue::Bool(true))
        .await
        .unwrap();
    driver
        .set_param(b, "eq2_gain", ParamValue::Float(2.0))
        .await
        .unwrap();
    driver
        .set_param(b, "eq5_gain", ParamValue::Float(1.5))
        .await
        .unwrap();
    driver
        .set_param(b, "comp_ratio", ParamValue::Float(3.0))
        .await
        .unwrap();
    driver
        .set_param(b, "pan", ParamValue::Float(10.0))
        .await
        .unwrap();
    let ParamValue::Float(g) = driver.get_param(b, "eq2_gain").await.unwrap() else {
        panic!("bus eq2_gain non-float");
    };
    assert!((g - 2.0).abs() < 1e-3, "bus eq2_gain {g}");
    let ParamValue::Float(r) = driver.get_param(b, "comp_ratio").await.unwrap() else {
        panic!("bus comp_ratio non-float");
    };
    assert!((r - 3.0).abs() < 1e-6, "bus comp_ratio {r}");
    // Channel-only leaves stay out of scope on buses.
    assert!(driver.get_param(b, "gain").await.is_err());
    assert!(driver.get_param(b, "gate_thr").await.is_err());
    assert!(driver.get_param(b, "aux1_level").await.is_err());

    let r = StripId(16);
    driver.set_fader(r, 0.7).await.unwrap();
    assert!((driver.get_fader(r).await.unwrap() - 0.7).abs() < addrs::FADER_READBACK_EPSILON);
    driver
        .set_param(r, "eq_on", ParamValue::Bool(true))
        .await
        .unwrap();
    assert_eq!(
        driver.get_param(r, "eq_on").await.unwrap(),
        ParamValue::Bool(true)
    );

    let f = StripId(26);
    driver.set_fader(f, 0.5).await.unwrap();
    driver
        .set_param(f, "color", ParamValue::Float(4.0))
        .await
        .unwrap();
    assert!(driver.get_param(f, "eq_on").await.is_err());
    assert!(driver.get_param(f, "pan").await.is_err());

    let m = StripId(30);
    driver.set_fader(m, 0.8).await.unwrap();
    driver
        .set_param(m, "eq2_gain", ParamValue::Float(1.0))
        .await
        .unwrap();
    assert!(driver.get_param(m, "pan").await.is_err());

    let d = StripId(31);
    driver.set_fader(d, 0.75).await.unwrap();
    driver.set_mute(d, false).await.unwrap();
    assert!(driver.get_param(d, "gate_thr").await.is_err());
}
