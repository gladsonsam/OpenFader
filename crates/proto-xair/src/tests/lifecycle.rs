use super::*;

/// Connect-time port sweep stays sub-second and honors explicit ports:
/// `probe_port` sees the strict fake, `has_explicit_port` gates the
/// sweep, and `connect()` with an explicit port dials it directly.
#[test]
fn explicit_port_detection() {
    assert!(has_explicit_port("192.168.1.20:10024"));
    assert!(has_explicit_port("192.168.1.20:10023"));
    assert!(!has_explicit_port("192.168.1.20"));
    assert!(!has_explicit_port("mixer.lan"));
}

#[tokio::test]
async fn connect_probe_sees_fake_fast() {
    let (addr, _fake) = FakeMixer::spawn().await;
    let ip = addr.ip().to_string();
    let t0 = std::time::Instant::now();
    assert!(probe_port(&ip, addr.port()).await);
    assert!(t0.elapsed() < addrs::CONNECT_PROBE_TIMEOUT);
    // Unused loopback port: connection-refused (or silence on wifi) —
    // either way the probe fails fast, never the steady-state 2 s.
    let probe = async {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sock.local_addr().unwrap().port()
    };
    let free = probe.await;
    let t0 = std::time::Instant::now();
    assert!(!probe_port("127.0.0.1", free).await);
    assert!(t0.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
async fn connect_explicit_port_dials_directly() {
    let (addr, _fake) = FakeMixer::spawn().await;
    let driver = XAirDriver::new();
    driver.connect(&addr.to_string()).await.unwrap();
    let info = driver.query_info().await.unwrap();
    assert_eq!(info.model, "XR18");
}
#[tokio::test]
async fn driver_channel_count_is_per_model() {
    let xr18 = XAirDriver::new();
    assert_eq!(xr18.channel_count(), 16);
    let x32kind = XAirDriver::with_model(addrs::XAirModel::X32Kind);
    assert_eq!(x32kind.channel_count(), 32);

    let (driver, _) = connected_driver().await;
    assert!(driver.set_fader(StripId(15), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(16), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(34), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(35), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(36), 0.5).await.is_err());
    assert!(driver.get_mute(StripId(99)).await.is_err());
    assert!(driver.set_bus_fader(6, 0.5).await.is_err());
    assert!(driver.get_bus_fader(6).await.is_err());
}

#[tokio::test]
async fn driver_rejects_out_of_range_strips() {
    let (driver, _) = connected_driver().await;
    assert!(driver.set_fader(StripId(32), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(35), 0.5).await.is_ok());
    assert!(driver.set_fader(StripId(36), 0.5).await.is_err());
    assert!(driver.get_mute(StripId(99)).await.is_err());
    assert!(driver.set_bus_fader(6, 0.5).await.is_err());
    assert!(driver.get_bus_fader(6).await.is_err());
}
