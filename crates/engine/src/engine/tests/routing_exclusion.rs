use super::*;

#[tokio::test]
async fn routing_exclusion_blocks_recalls_and_routes_but_not_mix_and_releases_on_error() {
    let engine = parked_engine();
    let guard = engine.routing_mutation_guard().await;
    let mut route_out = Vec::new();
    let mut recall_out = Vec::new();
    let route = engine.apply(
        Cmd::PatchClear {
            strip: 1,
            key: None,
            target: String::new(),
        },
        "test",
        &mut route_out,
    );
    let recall = engine.apply(
        Cmd::SceneRecall {
            n: 0,
            fade_ms: None,
            scope: None,
        },
        "test",
        &mut recall_out,
    );
    tokio::pin!(route, recall);
    assert!(tokio::time::timeout(Duration::from_millis(30), &mut route)
        .await
        .is_err());
    assert!(tokio::time::timeout(Duration::from_millis(30), &mut recall)
        .await
        .is_err());
    let mut out = Vec::new();
    assert!(tokio::time::timeout(
        Duration::from_secs(1),
        engine.apply(
            Cmd::Fader {
                ch: 1,
                value: 0.5,
                key: None
            },
            "test",
            &mut out
        )
    )
    .await
    .is_ok());
    drop(guard);
    let _ = tokio::time::timeout(Duration::from_secs(1), &mut route)
        .await
        .unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(1), &mut recall)
        .await
        .unwrap()
        .is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(1), engine.routing_mutation_guard())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn selector_session_owns_guard_and_rejects_stale_generation() {
    let engine = parked_engine();
    let session = engine.selector_session().await.unwrap();
    assert_eq!(session.identity().model, "sim");
    assert!(
        tokio::time::timeout(Duration::from_millis(30), engine.routing_mutation_guard())
            .await
            .is_err()
    );
    assert!(session.soundcheck_readback().await.is_err());
    let profile = session.soundcheck_profile().await.unwrap().unwrap();
    assert_eq!(profile.capture_channels, 32);
    engine.invalidate_connection();
    assert!(session.soundcheck_profile().await.is_err());
    assert!(session.soundcheck_readback().await.is_err());
    assert!(session
        .read_input_route(0)
        .await
        .unwrap_err()
        .to_string()
        .contains("stale"));
    drop(session);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), engine.routing_mutation_guard())
            .await
            .is_ok()
    );
}
