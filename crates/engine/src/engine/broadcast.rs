//! Broadcast operations.

use super::*;

impl Engine {
    /// Send an event to every connected client (skips closed mailboxes;
    /// the sweeper prunes them). Notifies [`tick_notify`](Engine::tick_notify)
    /// waiters (edge push tasks), one permit per call, coalesced by design.
    ///
    /// The event is serialized once and shared: one [`SharedEvent`] (one
    /// `serde_json` run, one backing allocation) is built here and an `Arc`
    /// clone is handed to each mailbox, so N clients pay one serialization
    /// instead of N. Per-client binary meter encoding still happens in the
    /// adapters (binary frames carry a per-connection sequence number).
    pub(super) fn broadcast(&self, event: Event) {
        let shared = SharedEvent::new(event);
        let clients = self.clients.read().unwrap();
        for entry in clients.values() {
            Self::push_to(entry, shared.clone());
        }
        drop(clients);
        self.notify.notify_one();
    }

    /// [`broadcast`] to everyone except one client (join roster updates).
    /// Notifies tick waiters like [`broadcast`](Engine::broadcast) does.
    /// Serialization is shared exactly like [`broadcast`].
    pub(super) fn broadcast_except(&self, event: Event, skip: &str) {
        let shared = SharedEvent::new(event);
        let clients = self.clients.read().unwrap();
        for (id, entry) in clients.iter() {
            if id != skip {
                Self::push_to(entry, shared.clone());
            }
        }
        drop(clients);
        self.notify.notify_one();
    }

    /// Unicast one event to a single client (direct replies, denials).
    /// Notifies tick waiters like [`broadcast`](Engine::broadcast) does.
    pub(super) fn unicast(&self, id: &str, event: Event) {
        if let Some(entry) = self.clients.read().unwrap().get(id) {
            Self::push_to(entry, SharedEvent::new(event));
        }
        self.notify.notify_one();
    }

    /// Deliver one frame to one mailbox (the single choke point behind
    /// [`broadcast`](Engine::broadcast), `broadcast_except` and `unicast`,
    /// plus the attach snapshot).
    ///
    /// Control frames (everything [`rt_slot_key`] rejects: state changes,
    /// params, roster, locks, errors, …) always queue reliably in order and
    /// are never dropped or coalesced. Meter-class frames queue directly on
    /// the fast path (no realtime backlog), but while a realtime frame is
    /// still queued they collapse into that mailbox's latest-only slots
    /// the newer frame supersedes the older same-key one, so a stalled
    /// client accumulates at most one queued realtime frame plus one slot
    /// set no matter how fast the ticker runs.
    pub(super) fn push_to(entry: &ClientEntry, frame: SharedEvent) {
        if let Some(key) = rt_slot_key(&frame) {
            let mut rt = entry.rt.lock().unwrap();
            if rt.inflight {
                rt.store(key, frame);
                return;
            }
            // Count before publishing: a receiver can consume immediately.
            entry.queued.fetch_add(1, Ordering::Relaxed);
            if entry.mailbox.send(frame).is_ok() {
                rt.inflight = true;
            } else {
                entry.queued.fetch_sub(1, Ordering::Relaxed);
            }
            return;
        }
        entry.queued.fetch_add(1, Ordering::Relaxed);
        if entry.mailbox.send(frame).is_err() {
            entry.queued.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Edge-adapter wakeup handle (P0): an edge-adapter `push_task` waits on
    /// this instead of a second 50 ms poll. Notified on every mailbox
    /// write (broadcasts, unicasts, attach snapshot); the 50 ms interval
    /// stays as a fallback ceiling. [`Notify`] stores one permit while no
    /// waiter is parked, so a broadcast between drain and wait still wakes
    /// the next wait (no lost wakeup, at most coalesced).
    pub fn tick_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    /// Allocate the next client id/label pair (`client-N` fallback).
    pub(super) fn alloc_client(&self, name: Option<&str>) -> (String, String) {
        let n = self.next_client.fetch_add(1, Ordering::Relaxed);
        let id = format!("client-{n}");
        let clean = name.map(str::trim).filter(|s| !s.is_empty());
        let label = clean.unwrap_or(&id).to_string();
        (id, label)
    }

    /// Detach a client: roster update + auto-release of its strip locks +
    /// drop of its pending focus work (so a gone client's flood never
    /// starves the next focus). Idempotent; also runs on [`ClientHandle`]
    /// drop.
    pub(super) fn remove_client(&self, id: &str) {
        let existed = self.clients.write().unwrap().remove(id).is_some();
        if !existed {
            return;
        }
        // Drop can run after the runtime is gone; asynchronous cleanup is best effort.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let drv = self.active_drv();
            let owner = id.to_string();
            runtime.spawn(async move {
                let _ = drv.set_spectrum_target(&owner, None).await;
            });
        }
        self.focus_queue
            .write()
            .unwrap()
            .retain(|(_, _, _, owner)| owner != id);
        self.broadcast(self.clients_message());
        let released = {
            let mut locks = self.locks.write().unwrap();
            let before = locks.len();
            locks.retain(|_, l| l.holder != id);
            locks.len() != before
        };
        if released {
            self.broadcast(self.locks_message_inner());
        }
    }

    /// Drop entries whose mailbox is closed (receiver dropped without
    /// detach); the next roster broadcast carries the pruned count.
    pub(super) fn prune_closed(&self) {
        let dead: Vec<String> = self
            .clients
            .read()
            .unwrap()
            .iter()
            .filter(|(_, e)| e.mailbox.is_closed())
            .map(|(id, _)| id.clone())
            .collect();
        for id in dead {
            self.remove_client(&id);
        }
    }

    /// Record a mutation write for `writers` tracking (last 10 s).
    pub(super) fn note_write(&self, id: &str) {
        let now = Instant::now();
        if let Some(info) = self.clients.write().unwrap().get_mut(id) {
            info.last_write = Some(now);
        }
    }

    pub(super) fn policy_of(&self, id: &str) -> Policy {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .map(|e| e.policy.clone())
            // A detached writer must not regain privileges in a guarded callback.
            .unwrap_or_else(|| Policy::musician([]))
    }

    /// Current `clients` event (`count` + `writers` with a mutation in the
    /// last 10 s, sorted for deterministic snapshots).
    pub(super) fn clients_message(&self) -> Event {
        let clients = self.clients.read().unwrap();
        let now = Instant::now();
        let mut writers: Vec<String> = clients
            .iter()
            .filter(|(_, c)| {
                c.last_write
                    .is_some_and(|t| now.duration_since(t).as_secs() < 10)
            })
            .map(|(id, _)| id.clone())
            .collect();
        writers.sort();
        Event::Clients {
            count: clients.len(),
            writers,
        }
    }

    /// Drop expired holds. Returns true when anything was removed.
    pub(super) fn prune_expired_locks(&self) -> bool {
        let now = Instant::now();
        let mut locks = self.locks.write().unwrap();
        let before = locks.len();
        locks.retain(|_, l| now < l.expires_at);
        locks.len() != before
    }

    pub(super) fn lock_holder(&self, ch: u16) -> Option<String> {
        let locks = self.locks.read().unwrap();
        let entry = locks.get(&ch)?;
        if Instant::now() >= entry.expires_at {
            return None;
        }
        Some(entry.holder.clone())
    }

    /// Refresh expiry on every non-expired hold owned by `writer`.
    pub(super) fn touch_holder_locks(&self, writer: &str) {
        let deadline = Instant::now() + self.lock_ttl;
        let now = Instant::now();
        let mut locks = self.locks.write().unwrap();
        for entry in locks.values_mut() {
            if entry.holder == writer && now < entry.expires_at {
                entry.expires_at = deadline;
            }
        }
    }

    /// Current `locks` event. Leave expired entries for the sweeper or
    /// command path to remove and broadcast; a snapshot must not consume an
    /// expiry silently before other clients are notified.
    pub(super) fn locks_message(&self) -> Event {
        self.locks_message_inner()
    }

    /// Build the `locks` event from the current table. Expired entries are
    /// skipped defensively even if pruning was skipped.
    pub(super) fn locks_message_inner(&self) -> Event {
        let locks = self.locks.read().unwrap();
        let now = Instant::now();
        let mut holds = std::collections::BTreeMap::new();
        let mut chs: Vec<u16> = locks.keys().copied().collect();
        chs.sort_unstable();
        for ch in chs {
            if let Some(entry) = locks.get(&ch) {
                if now < entry.expires_at {
                    holds.insert(ch.to_string(), entry.holder.clone());
                }
            }
        }
        Event::Locks { holds }
    }

    /// Denied-write error event (`locked_by` names the holder).
    pub(super) fn locked_error(ch: u16, holder: &str) -> Event {
        Event::Error {
            message: format!("strip {ch} locked by {holder}"),
            locked_by: Some(holder.to_string()),
            ch: Some(ch),
        }
    }
}
