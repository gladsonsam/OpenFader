use super::*;

/// Clear-solo action: fire-and-forget set ACKed by silence, query reads
/// back int 0 (nothing soloed) — the live-desk shape.
#[tokio::test]
async fn driver_clear_solo_fire_and_forget() {
    let (driver, _) = connected_driver().await;
    driver.clear_solo().await.unwrap();
}

/// Rig-gated solo write-path discovery (env `OPENFADER_RIG=1`, desk
/// address via `OPENFADER_RIG_ADDR`; skipped otherwise so CI stays
/// green). Uses muted
/// ch16 only, read -> write -> readback -> clear -> verify. Reports
/// which `/-stat/solosw/NN` numbering the desk honors (1-based `16`
/// vs 0-based `15`) — run with `-- --nocapture` to see the verdict.
/// PFL only, cleared immediately; never run while the rig is live.
#[tokio::test]
async fn rig_solo_write_path() {
    if std::env::var("OPENFADER_RIG").as_deref() != Ok("1") {
        return;
    }
    let driver = XAirDriver::new();
    let rig = std::env::var("OPENFADER_RIG_ADDR").unwrap_or_else(|_| "192.168.1.120".to_string());
    driver.connect(&format!("{rig}:10024")).await.unwrap();
    let bank = async || {
        let guard = driver.session().await.unwrap();
        let reply = guard
            .as_ref()
            .unwrap()
            .query("/-stat/solosw")
            .await
            .unwrap();
        reply
            .args
            .iter()
            .map(|a| match a {
                rosc::OscType::Int(i) => *i,
                _ => -999,
            })
            .collect::<Vec<_>>()
    };
    let set_idx = async |nn: u8, v: i32| {
        let guard = driver.session().await.unwrap();
        guard
            .as_ref()
            .unwrap()
            .send(
                &format!("/-stat/solosw/{nn:02}"),
                vec![rosc::OscType::Int(v)],
            )
            .await
            .unwrap();
    };
    let b0 = bank().await;
    println!("rig solosw bank len: {}", b0.len());
    println!(
        "baseline nonzero: {:?}",
        b0.iter()
            .enumerate()
            .filter(|(_, &v)| v != 0)
            .collect::<Vec<_>>()
    );
    // Hypothesis A: 1-based (ch16 -> /16, bank idx 15).
    set_idx(16, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let ba = bank().await;
    println!(
        "after /16=1: {:?}",
        ba.iter()
            .enumerate()
            .filter(|(_, &v)| v != 0)
            .collect::<Vec<_>>()
    );
    driver.clear_solo().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    // Hypothesis B: 0-based (ch16 -> /15, bank idx 15).
    set_idx(15, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let bb = bank().await;
    println!(
        "after /15=1: {:?}",
        bb.iter()
            .enumerate()
            .filter(|(_, &v)| v != 0)
            .collect::<Vec<_>>()
    );
    driver.clear_solo().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let b1 = bank().await;
    assert!(b1.iter().all(|&v| v == 0), "rig solos not cleared: {b1:?}");
}

/// Main LR mute (`/lr/mix/on`): same-value writes round-trip bit-exact
/// against the strict fake (the live desk answers this leaf, rig-verified
/// XR18 FW 1.20 2026-10-18: reads `1` unmuted).
#[tokio::test]
async fn driver_main_mute_roundtrip() {
    let (driver, _) = connected_driver().await;
    driver.set_main_mute(false).await.unwrap();
    assert!(!driver.get_main_mute().await.unwrap());
    driver.set_main_mute(true).await.unwrap();
    assert!(driver.get_main_mute().await.unwrap());
}

/// Per-channel solo (`/-stat/solosw/NN` set, `/-stat/solosw` bank
/// readback): round-trips against the strict fake's 54-int bank. Non-
/// channel strips bail locally (no verified desk leaf — never faked).
#[tokio::test]
async fn driver_solo_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert!(!driver.get_solo(StripId(15)).await.unwrap());
    driver.set_solo(StripId(15), true).await.unwrap();
    assert!(driver.get_solo(StripId(15)).await.unwrap());
    assert!(!driver.get_solo(StripId(0)).await.unwrap());
    driver.clear_solo().await.unwrap();
    // Invalid strips remain unsupported; Main requires the guarded target path.
    assert!(driver.set_solo(StripId(36), true).await.is_err());
    assert!(driver.set_solo(StripId(30), true).await.is_err());
    assert!(driver.get_solo(StripId(36)).await.is_err());
}

/// Solo monitoring globals loopback over UDP (`crate::solo`,
/// rig-verified 2026-10-19): dim/mute/level/source round-trip through
/// the global driver methods; untouched reads match the rig (dim 0,
/// mute 0, level 0.75, source 3).
#[tokio::test]
async fn driver_solo_globals_roundtrip() {
    let (driver, _) = connected_driver().await;
    assert!(!driver.get_solo_dim().await.unwrap());
    assert!(!driver.get_solo_mute().await.unwrap());
    assert!((driver.get_solo_level().await.unwrap() - 0.75).abs() < 1e-6);
    assert_eq!(driver.get_solo_source().await.unwrap(), 3);
    driver.set_solo_dim(true).await.unwrap();
    assert!(driver.get_solo_dim().await.unwrap());
    driver.set_solo_dim(false).await.unwrap();
    assert!(!driver.get_solo_dim().await.unwrap());
    driver.set_solo_mute(true).await.unwrap();
    assert!(driver.get_solo_mute().await.unwrap());
    driver.set_solo_mute(false).await.unwrap();
    assert!(!driver.get_solo_mute().await.unwrap());
    driver.set_solo_level(0.5).await.unwrap();
    assert!((driver.get_solo_level().await.unwrap() - 0.5).abs() < 1e-6);
    assert!(driver.set_solo_level(1.1).await.is_err());
    assert!(driver.set_solo_level(f32::NAN).await.is_err());
    driver.set_solo_source(0).await.unwrap();
    assert_eq!(driver.get_solo_source().await.unwrap(), 0);
    driver.set_solo_source(14).await.unwrap();
    assert_eq!(driver.get_solo_source().await.unwrap(), 14);
    assert!(driver.set_solo_source(15).await.is_err());
    // Globals are not strip params (out of strip caps, never polled).
    assert!(!XAIR_CAPABILITIES.contains(&"solo_dim"));
    assert!(!XAIR_CAPABILITIES.contains(&"solo_mute"));
    assert!(!XAIR_CAPABILITIES.contains(&"solo_level"));
    assert!(!XAIR_CAPABILITIES.contains(&"solo_source"));
    assert!(driver.get_param(StripId(0), "solo_dim").await.is_err());
    assert!(driver.get_param(StripId(30), "solo_dim").await.is_err());
}
