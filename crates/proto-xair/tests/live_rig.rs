//! Live-rig proof for the six mechanism items (owner XR18 on the LAN).
//!
//! IGNORED by default (`cargo test --workspace` skips these): run only on
//! a LAN with an XR18, desk address via `OPENMIX_RIG_ADDR` (default
//! `192.168.1.120`), with `OPENMIX_LIVE_RIG=1 cargo test -p openmix-proto-xair
//! --test live_rig -- --ignored`. Reads plus same-value roundtrips only,
//! except the single `/-action/clearsolo` fire-and-forget (speakers-safe,
//! monitoring-side only) and read-only measurement queries. NEVER run from
//! CI or off-LAN: every test fails closed without the env var.

use openmix_core::{MixerDriver, ParamValue, StripId};
use openmix_proto_xair::XAirDriver;

fn rig_addr() -> String {
    std::env::var("OPENMIX_RIG_ADDR").unwrap_or_else(|_| "192.168.1.120".to_string())
}

fn live() -> bool {
    std::env::var("OPENMIX_LIVE_RIG").as_deref() == Ok("1")
}

async fn connected() -> XAirDriver {
    let d = XAirDriver::new();
    // Bare IP (no `:port`): exercises the connect-time 10024 -> 10023 sweep.
    d.connect(&rig_addr()).await.expect("rig connect (sweep) failed");
    d
}

/// Item 1: send-level / headamp-gain / color decode on the live desk.
#[tokio::test]
#[ignore]
async fn live_inbound_leaves_read() {
    if !live() {
        return;
    }
    let d = connected().await;
    // `/ch/02/mix/01/level` == aux1_level: valid dB in the send law range
    // (desk state drifts — no exact-value assert on non-baseline leaves).
    let v = d.get_param(StripId(1), "aux1_level").await.unwrap();
    assert!(matches!(v, ParamValue::Float(f) if (-90.0..=10.0).contains(&f)), "aux1_level: {v:?}");
    // `/headamp/02/gain` answers (wire 0..1 -> dB).
    let v = d.get_param(StripId(1), "gain").await.unwrap();
    assert!(matches!(v, ParamValue::Float(f) if (-12.0..=60.0).contains(&f)), "gain: {v:?}");
    // `/ch/16/config/color` answers (baseline §7: 0).
    let v = d.get_param(StripId(15), "color").await.unwrap();
    assert!(matches!(v, ParamValue::Float(_)), "color: {v:?}");
}

/// Item 2: mute-group masters 1..4 answer; same-value write round-trips.
#[tokio::test]
#[ignore]
async fn live_mute_group_master() {
    if !live() {
        return;
    }
    let d = connected().await;
    assert_eq!(d.mute_group_count(), 4);
    for g in 0..4 {
        let cur = d.get_mute_group(g).await.unwrap();
        // Same-value roundtrip only (never toggles: could mute members).
        d.set_mute_group(g, cur).await.unwrap();
        assert_eq!(d.get_mute_group(g).await.unwrap(), cur);
    }
}

/// Item 5: single clearsolo fire-and-forget; desk stays responsive after.
#[tokio::test]
#[ignore]
async fn live_clear_solo() {
    if !live() {
        return;
    }
    let d = connected().await;
    d.clear_solo().await.unwrap();
    // Desk still answers immediately after the fire-and-forget.
    let f = d.get_fader(StripId(1)).await.unwrap();
    assert!((f - 0.0).abs() < 1e-6, "ch2 fader moved: {f}");
}
