//! Latest connection intent wins, including disconnect during a slow handshake.
//! Mixer writes do not enter this lane and are never retried here.
use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Mutex, Notify};

#[derive(Default)]
pub(crate) struct ConnectionLifecycle {
    intent: AtomicU64,
    changed: Notify,
    lane: Mutex<()>,
}

impl ConnectionLifecycle {
    /// Cancel older connection work before waiting for its lane. Dropping the
    /// old future retires its temporary provider session; the winning intent
    /// owns normal engine generation/cache cleanup and status publication.
    pub(crate) async fn run<T>(
        &self,
        work: impl Future<Output = T>,
        on_cancel: impl FnOnce(),
    ) -> Option<T> {
        let intent = self.intent.fetch_add(1, Ordering::AcqRel) + 1;
        self.changed.notify_waiters();
        let _lane = self.lane.lock().await;
        let changed = self.changed.notified();
        tokio::pin!(changed);
        // Register before comparing, so a new intent cannot hide between the
        // comparison and the first poll of the cancellation notification.
        changed.as_mut().enable();
        if self.intent.load(Ordering::Acquire) != intent {
            return None;
        }
        tokio::select! {
            biased;
            _ = changed => { on_cancel(); None },
            result = work => Some(result),
        }
    }
}
