//! Offline-first sync replay against the SimDriver.
use openmix_core::sync::OpQueue;
use openmix_core::{MixerDriver, StripId};
use openmix_sim::SimDriver;

#[tokio::test]
async fn replay_applies_lww_and_logs_conflict() {
    let d = SimDriver::new();
    d.connect("sim://local").await.unwrap();
    let mut q = OpQueue::new();
    // 3 ops queued "offline".
    q.push_fader(StripId(0), 0.2).unwrap();
    q.push_fader(StripId(0), 0.8).unwrap();
    q.push_mute(StripId(1), true);
    assert_eq!(q.len(), 3);

    let report = q.replay(&d).await.unwrap();
    assert!(q.is_empty());
    assert_eq!(report.applied, 2);
    assert_eq!(report.coalesced, 1);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].dropped_seq, 0);
    assert_eq!(report.conflicts[0].kept_seq, 1);

    // Final state = last write per target.
    assert_eq!(d.get_fader(StripId(0)).await.unwrap(), 0.8);
    assert!(d.get_mute(StripId(1)).await.unwrap());
}

#[tokio::test]
async fn replay_empty_queue_is_noop() {
    let d = SimDriver::new();
    d.connect("sim://local").await.unwrap();
    let mut q = OpQueue::new();
    let report = q.replay(&d).await.unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(report.coalesced, 0);
}
