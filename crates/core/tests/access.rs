//! Access-role enforcement against the SimDriver.
use openfader_core::access::{CheckedDriver, Policy};
use openfader_core::{MixerDriver, StripId};
use openfader_sim::SimDriver;

async fn driven() -> SimDriver {
    let d = SimDriver::new();
    d.connect("sim://local").await.unwrap();
    d
}

fn musician(d: &SimDriver) -> CheckedDriver<'_, SimDriver> {
    CheckedDriver::new(d, Policy::musician([0, 1]))
}

#[tokio::test]
async fn musician_blocked_from_main_lr() {
    let d = driven().await;
    let checked = musician(&d);
    let err = checked.set_main_fader(0.5).await.unwrap_err();
    assert!(err.to_string().contains("main LR"), "{err}");
    // Reads still pass through.
    assert!(checked.get_main_fader().await.is_ok());
}

#[tokio::test]
async fn musician_capped_at_minus_10db() {
    let d = driven().await;
    let checked = musician(&d);
    let err = checked.set_fader(StripId(0), 0.9).await.unwrap_err();
    assert!(err.to_string().contains("cap"), "{err}");
    checked.set_fader(StripId(0), 0.5).await.unwrap();
    assert_eq!(d.get_fader(StripId(0)).await.unwrap(), 0.5);
}

#[tokio::test]
async fn musician_denied_strips_rejected() {
    let d = driven().await;
    let checked = musician(&d);
    let err = checked.set_fader(StripId(5), 0.5).await.unwrap_err();
    assert!(err.to_string().contains("not allowed"), "{err}");
    let err = checked.set_mute(StripId(5), true).await.unwrap_err();
    assert!(err.to_string().contains("not allowed"), "{err}");
    // Allow-listed strips work.
    checked.set_mute(StripId(1), true).await.unwrap();
}

#[tokio::test]
async fn musician_cannot_recall_or_rename() {
    let d = driven().await;
    let checked = musician(&d);
    let snap = d.snapshot().await.unwrap();
    assert!(checked.recall(&snap).await.is_err());
    assert!(checked.set_name(StripId(0), "vox").await.is_err());
}

#[tokio::test]
async fn admin_and_engineer_have_full_access() {
    let d = driven().await;
    for policy in [Policy::admin(), Policy::engineer()] {
        let checked = CheckedDriver::new(&d, policy);
        checked.set_fader(StripId(0), 1.0).await.unwrap();
        checked.set_main_fader(0.9).await.unwrap();
        checked.set_name(StripId(0), "vox").await.unwrap();
        let snap = checked.snapshot().await.unwrap();
        checked.recall(&snap).await.unwrap();
    }
}
