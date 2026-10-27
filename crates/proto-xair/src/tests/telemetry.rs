use super::*;

/// Meter subscription fills the cache that `poll_meters` serves
/// strip-ordered: the strict fake streams `[0.1, 0.5, 0.9]` (a legacy
/// blob with no count word, so transport keeps the plain LE-i16 path),
/// which must land verbatim on strips 0..3 while the aux strip stays
/// silent (a 3-level frame has no idx 26).
#[tokio::test]
async fn driver_meter_cache_fills() {
    let (driver, _) = connected_driver().await;
    assert!(driver.poll_meters().levels.iter().all(|l| *l == 0.0));
    let _rx = driver.subscribe_meters().await.unwrap();
    let t0 = std::time::Instant::now();
    loop {
        if driver.poll_meters().levels.iter().any(|l| *l > 0.0)
            || t0.elapsed() > std::time::Duration::from_secs(3)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let levels = driver.poll_meters().levels;
    assert_eq!(levels.len(), XAIR_STRIPS);
    assert!((levels[0] - 0.1).abs() < 1e-4);
    assert!((levels[1] - 0.5).abs() < 1e-4);
    assert!((levels[2] - 0.9).abs() < 1e-4);
    assert_eq!(levels[35], 0.0);
}

/// Desk-broadcast ingest for EQ over the real `connect()` path: an
/// unsolicited `/ch/01/eq/1/g` datagram from the fake (no query asked
/// for it — the desk-broadcast shape) must surface on the driver's
/// update channel as `eq1_gain` in engineering units, like the polled
/// `get_param` arm. Covers the connect-spawned ingest wiring, not just
/// the pure `parse_inbound` decode above.
#[tokio::test]
async fn inbound_eq_gain_reaches_update_channel() {
    let (addr, fake) = FakeMixer::spawn().await;
    let driver = XAirDriver::new();
    driver.connect(&addr.to_string()).await.unwrap();
    let mut rx = driver
        .subscribe_updates()
        .expect("driver has update channel");
    // One query so the fake learns our peer (connect's best-effort
    // /info ping already did, but be explicit), then fire the
    // unsolicited broadcast the desk would emit on a surface move.
    let _ = driver.get_fader(StripId(0)).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    fake.broadcast("/ch/01/eq/1/g", vec![OscType::Float(0.75)])
        .await;
    let deadline = std::time::Duration::from_secs(2);
    let t0 = std::time::Instant::now();
    loop {
        let update = tokio::time::timeout(deadline - t0.elapsed(), rx.recv())
            .await
            .expect("eq broadcast not ingested in time")
            .expect("update channel closed");
        if update.strip.0 == 0 && update.param == "eq1_gain" {
            assert!(
                matches!(update.value, ParamValue::Float(v) if (v - 7.5).abs() < 1e-5),
                "eq1_gain decoded off-codec: {:?}",
                update.value
            );
            return;
        }
    }
}

/// Meter subscription uses the documented `/meters` + stream-id-string
/// form (T4): the fake streams blobs only after that subscribe, and the
/// test asserts both the subscribe shape and the parsed frames.
#[tokio::test]
async fn driver_meter_subscribe_receives_blob() {
    let (driver, fake) = connected_driver().await;
    let mut rx = driver.subscribe_meters().await.unwrap();
    // The desk streams at ~50ms; expect at least two level frames. The
    // fake also streams ALL DYN blobs now (same subscribe call), which
    // the shared hub surfaces as 41-level frames — skipped here, the
    // DYN path has its own test below.
    let mut seen = 0;
    for _ in 0..8 {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("meter frame in time")
            .unwrap();
        if frame.levels.len() != 3 {
            continue;
        }
        assert!((frame.levels[1] - 0.5).abs() < 0.01);
        seen += 1;
        if seen == 2 {
            break;
        }
    }
    assert_eq!(seen, 2, "expected two level frames");
    let sub = fake.meter_subscribe.lock().unwrap();
    let sub = sub.as_ref().expect("fake saw no meter subscribe");
    assert_eq!(sub.addr, addrs::METER_REQUEST);
    assert_eq!(
        sub.args,
        vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())]
    );
}

/// ALL DYN subscribe uses the same documented `/meters` + string-id
/// form with the `/meters/6` id (Behringer X AIR OSC doc fw 1.11).
#[tokio::test]
async fn driver_meter_dyn_subscribe_shape() {
    let (driver, fake) = connected_driver().await;
    let _rx = driver.subscribe_meters().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let sub = fake.dyn_subscribe.lock().unwrap();
    let sub = sub.as_ref().expect("fake saw no dyn subscribe");
    assert_eq!(sub.addr, addrs::METER_REQUEST);
    assert_eq!(
        sub.args,
        vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())]
    );
}

/// Renewal loop: `/xremote` + both `/meters` resubscribes go out on
/// schedule (INFERRED ~8 s production interval;
/// [`addrs::METER_RENEW_INTERVAL`]). Short 50 ms interval here keeps the
/// test fast; the fake tolerates resubscribes by restarting its blob
/// burst and counting every hit.
#[tokio::test]
async fn meter_renewal_resubscribes_on_schedule() {
    use std::time::Duration;
    assert_eq!(
        addrs::METER_RENEW_INTERVAL,
        Duration::from_secs(8),
        "production renewal stays ~8s (INFERRED until rig confirms)"
    );
    let (driver, fake) = connected_driver().await;
    // Session keepalive (`/xremote` every 5 s) fires immediately too —
    // reset its counter so only the renewal loop is measured below.
    tokio::time::sleep(Duration::from_millis(50)).await;
    *fake.xremote_count.lock().unwrap() = 0;
    *fake.meter_count.lock().unwrap() = 0;
    *fake.dyn_count.lock().unwrap() = 0;

    let _rx = driver
        .subscribe_meters_with_interval(Duration::from_millis(50))
        .await
        .unwrap();
    // Initial subscribe + ~4 renewals in 250 ms.
    tokio::time::sleep(Duration::from_millis(250)).await;
    let levels = *fake.meter_count.lock().unwrap();
    let dyns = *fake.dyn_count.lock().unwrap();
    let keepalives = *fake.xremote_count.lock().unwrap();
    assert!(
        levels >= 3,
        "expected >=3 /meters/1 subscribes (initial + renewals), saw {levels}"
    );
    assert!(
        dyns >= 3,
        "expected >=3 /meters/6 subscribes (initial + renewals), saw {dyns}"
    );
    assert!(
        keepalives >= 3,
        "expected >=3 /xremote renewals, saw {keepalives}"
    );
}

/// GR end-to-end at the codec level: the fake emits the documented
/// `/meters/6` shape over UDP loopback, the driver ingests the decoded
/// message and serves per-channel dB reduction via the param path.
#[tokio::test]
async fn driver_dyn_gr_param_loopback() {
    let (driver, fake) = connected_driver().await;
    // No blob yet: no-data error (the UI keeps the static scale), never
    // a fabricated zero.
    assert!(driver.get_param(StripId(0), "gate_gr").await.is_err());
    assert!(driver.get_param(StripId(0), "comp_gr").await.is_err());

    let mut m = dyn_gr::DynMeters {
        gate: [0.0; dyn_gr::DYN_CHANNELS],
        comp: [0.0; dyn_gr::DYN_CHANNELS],
        bus: [0.0; dyn_gr::DYN_BUSES],
        lr: 0.0,
    };
    m.gate[0] = 6.0;
    m.comp[0] = 12.0;
    m.comp[15] = 3.5;
    let wire = osc::encode_message(
        addrs::METER_DYN_SUBSCRIBE,
        vec![OscType::Blob(dyn_gr::build_dyn_blob(&m))],
    )
    .unwrap();
    let rx_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    fake.socket
        .send_to(&wire, rx_sock.local_addr().unwrap())
        .await
        .unwrap();
    let mut buf = vec![0u8; 65535];
    let (n, _) = rx_sock.recv_from(&mut buf).await.unwrap();
    let msgs = osc::flatten(osc::decode_packet(&buf[..n]).unwrap());
    assert_eq!(msgs.len(), 1);
    driver.ingest_dyn_message(&msgs[0]).await.unwrap();

    let gate = driver.get_param(StripId(0), "gate_gr").await.unwrap();
    let comp = driver.get_param(StripId(0), "comp_gr").await.unwrap();
    let comp16 = driver.get_param(StripId(15), "comp_gr").await.unwrap();
    let ParamValue::Float(gate) = gate else {
        panic!("gate_gr not a float");
    };
    let ParamValue::Float(comp) = comp else {
        panic!("comp_gr not a float");
    };
    let ParamValue::Float(comp16) = comp16 else {
        panic!("comp_gr not a float");
    };
    assert!((gate - 6.0).abs() < 0.01, "gate_gr {gate}");
    assert!((comp - 12.0).abs() < 0.01, "comp_gr {comp}");
    assert!((comp16 - 3.5).abs() < 0.01, "comp_gr ch16 {comp16}");
    let ParamValue::Float(idle) = driver.get_param(StripId(1), "gate_gr").await.unwrap() else {
        panic!("gate_gr not a float");
    };
    assert_eq!(idle, 0.0);
    assert!(driver.get_param(StripId(16), "gate_gr").await.is_err());
    assert!(driver.get_param(StripId(0), "nope").await.is_err());
    let ParamValue::Float(fader) = driver.get_param(StripId(0), "fader").await.unwrap() else {
        panic!("fader not a float");
    };
    assert!((fader - 0.75).abs() < addrs::FADER_READBACK_EPSILON);
}

/// Non-DYN messages never poison the GR cache: wrong address and
/// blobless payloads bail, and the cache stays empty.
#[tokio::test]
async fn driver_dyn_ingest_rejects_other_shapes() {
    let (driver, _) = connected_driver().await;
    let other = OscMessage {
        addr: addrs::METER_SUBSCRIBE.to_string(),
        args: vec![OscType::Blob(vec![0u8; 8])],
    };
    assert!(driver.ingest_dyn_message(&other).await.is_err());
    let blobless = OscMessage {
        addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
        args: vec![],
    };
    assert!(driver.ingest_dyn_message(&blobless).await.is_err());
    assert!(driver.last_dyn().await.is_none());
}

/// Garbage `/meters/6` payloads (truncated venue datagrams, corrupted
/// blobs, non-blob args) must bail with `Err`, never panic and never
/// poison the GR cache; a later valid blob still ingests cleanly.
#[tokio::test]
async fn driver_dyn_ingest_rejects_garbage_never_panics() {
    let (driver, _) = connected_driver().await;
    let mut m = dyn_gr::DynMeters {
        gate: [0.0; dyn_gr::DYN_CHANNELS],
        comp: [0.0; dyn_gr::DYN_CHANNELS],
        bus: [0.0; dyn_gr::DYN_BUSES],
        lr: 0.0,
    };
    m.gate[0] = 6.0;
    let good = dyn_gr::build_dyn_blob(&m);
    for len in 0..good.len() {
        let msg = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args: vec![OscType::Blob(good[..len].to_vec())],
        };
        assert!(
            driver.ingest_dyn_message(&msg).await.is_err(),
            "truncated len {len} ingested"
        );
    }
    for fill in [0x00u8, 0xABu8, 0xFFu8] {
        let msg = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args: vec![OscType::Blob(vec![fill; dyn_gr::DYN_BLOB_LEN])],
        };
        assert!(driver.ingest_dyn_message(&msg).await.is_err());
    }
    for args in [
        vec![OscType::Float(0.5)],
        vec![OscType::String("garbage".into())],
        vec![OscType::Int(-1), OscType::Blob(vec![0xABu8; 10])],
    ] {
        let msg = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args,
        };
        assert!(driver.ingest_dyn_message(&msg).await.is_err());
    }
    let stringy = OscMessage {
        addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
        args: vec![OscType::String("garbage".into())],
    };
    assert!(driver.ingest_dyn_message(&stringy).await.is_err());
    assert!(driver.last_dyn().await.is_none());
    let valid = OscMessage {
        addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
        args: vec![OscType::Blob(good)],
    };
    driver.ingest_dyn_message(&valid).await.unwrap();
    assert_eq!(driver.last_dyn().await.unwrap().gate[0], 6.0);
}
