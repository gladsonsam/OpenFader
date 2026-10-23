//! Scenes/shows recall against the SimDriver (integration: one core build).
use openfader_core::show::Scene;
use openfader_core::{MixerDriver, StripId};
use openfader_sim::SimDriver;

async fn driven() -> SimDriver {
    let d = SimDriver::new();
    d.connect("sim://local").await.unwrap();
    d
}

#[tokio::test]
async fn capture_roundtrip() {
    let d = driven().await;
    d.set_fader(StripId(0), 0.3).await.unwrap();
    d.set_mute(StripId(1), true).await.unwrap();
    let scene = Scene::capture("verse", &d.snapshot().await.unwrap());
    assert_eq!(scene.strip(StripId(0)).unwrap().fader, 0.3);
    assert!(scene.strip(StripId(1)).unwrap().mute);
    assert!(scene.main_fader.is_none());
}

#[tokio::test]
async fn recall_jump_applies_targets() {
    let d = driven().await;
    let mut scene = Scene::capture("chorus", &d.snapshot().await.unwrap());
    scene.strips[0].fader = 0.9;
    scene.strips[1].mute = true;
    scene.main_fader = Some(0.6);
    scene.recall_to(&d, 0).await.unwrap();
    assert_eq!(d.get_fader(StripId(0)).await.unwrap(), 0.9);
    assert!(d.get_mute(StripId(1)).await.unwrap());
    assert_eq!(d.get_main_fader().await.unwrap(), 0.6);
}

#[tokio::test]
async fn recall_fade_converges_to_targets() {
    let d = driven().await;
    let mut scene = Scene::capture("fade", &d.snapshot().await.unwrap());
    for s in scene.strips.iter_mut().take(4) {
        s.fader = 0.1;
    }
    // Short fade keeps the test fast; final snap must hit targets exactly.
    scene.recall_to(&d, 30).await.unwrap();
    for i in 0..4 {
        assert_eq!(d.get_fader(StripId(i)).await.unwrap(), 0.1);
    }
}

#[tokio::test]
async fn trait_default_recall_scene_matches_inherent() {
    let d = driven().await;
    let mut scene = Scene::capture("cue", &d.snapshot().await.unwrap());
    scene.strips[2].fader = 0.42;
    d.recall_scene(&scene, 0).await.unwrap();
    assert_eq!(d.get_fader(StripId(2)).await.unwrap(), 0.42);
}
