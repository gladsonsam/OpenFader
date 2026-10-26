//! Clients operations.

use super::*;

impl Engine {
    /// Attach a client: allocate its id, register it, and queue the
    /// initial snapshot (`you`, `state`, `shows`, `clients`, `locks`,
    /// slim `caps` (essentials lane, the full catalogue follows on the
    /// client's first `hello`, see
    /// [`caps_slim_message`](Engine::caps_slim_message)), `devices`,
    /// `link`, `info`?, `scenes`, `fx`, `routing`,
    /// master-section frames (`bus`/`subgroup`/`mute_group` + sparse
    /// assigns + `main` + sparse `solo`), then one `params` frame per
    /// cached channel, then the read-only `native_snapshots` frame, then
    /// (device mode only) the `local_library_status` frame) into the new
    /// mailbox before any live broadcast can reach it. The join roster
    /// update goes to the already-connected siblings only.
    ///
    /// Dropping the returned handle detaches the client (roster update +
    /// lock auto-release); call [`ClientHandle::detach`] for an explicit
    /// async detach. Starts the background poller/sweeper/meter tasks on
    /// first call (requires a Tokio runtime).
    pub async fn attach(self: &Arc<Self>, label: Option<String>) -> ClientHandle {
        self.ensure_tasks();
        let (id, clean) = self.alloc_client(label.as_deref());
        let (tx, rx) = mpsc::unbounded_channel();
        // A connection can change while the async snapshot is being read.
        // Retry before registering so no old rows follow the new epoch.
        loop {
            let generation = self.connection_gen.load(Ordering::Acquire);

            // Snapshot the world before registering, so no live broadcast can
            // overtake the initial frames in the newcomer's mailbox. The async
            // pieces are independent reads, join them so the wall time is the
            // slowest, not the sum (on hardware drivers these are desk
            // queries); the sync pieces (shows/locks/caps/devices/link/params)
            // are lock-local and stay serial.
            let snapshot_started = Instant::now();
            // Per-mailbox realtime coalescing state: created per attach
            // attempt (a generation retry discards it with the snapshot).
            let rt = Arc::new(std::sync::Mutex::new(RtSlots::default()));
            let queued = Arc::new(AtomicUsize::new(0));
            let you = Event::You { id: id.clone() };
            let shows = self.shows_message();
            let locks = self.locks_message();
            let caps = self.caps_slim_message();
            let devices = self.devices_message();
            let link = self.link_message_cached();
            // Sync lock-local read (no I/O): the read-only native slot
            // frame for this snapshot.
            let native = self.native_snapshots_message();
            let (state, info, scenes, _fx, routing, masters) = tokio::join!(
                self.state_message(None),
                async { self.info_message().await.ok() },
                async { self.scenes_message() },
                self.fx_message(),
                self.routing_message(),
                self.masters_snapshot()
            );

            {
                // No async work under these locks. Prefix, current cache replay
                // and registration are indivisible with respect to live delivery.
                let sync = self.sync_state.lock().unwrap();
                if self.connection_gen.load(Ordering::Acquire) != generation {
                    continue;
                }
                let params = self.params_snapshot();
                let fx = self.fx_message_cached();
                let mut clients = self.clients.write().unwrap();
                clients.insert(
                    id.clone(),
                    ClientEntry {
                        mailbox: tx.clone(),
                        rt: Arc::clone(&rt),
                        queued: Arc::clone(&queued),
                        label: clean.clone(),
                        connected_at: Instant::now(),
                        last_write: None,
                        policy: Policy::admin(),
                        binary_meters: false,
                        taps: false,
                        caps_full: false,
                    },
                );
                let mut writers: Vec<_> = clients
                    .iter()
                    .filter(|(_, c)| c.last_write.is_some_and(|t| t.elapsed().as_secs() < 10))
                    .map(|(id, _)| id.clone())
                    .collect();
                writers.sort();
                let roster = Event::Clients {
                    count: clients.len(),
                    writers,
                };
                let entry = clients.get(&id).expect("client just inserted");
                // Read-only native slots ride after the param plane (past
                // every position existing snapshots assert) and before the
                // epoch barrier.
                for frame in [you, state, shows, roster, locks, caps, devices, link]
                    .into_iter()
                    .chain(info)
                    .chain([scenes, fx, routing])
                    .chain(masters)
                    .chain(params)
                    .chain(std::iter::once(native))
                    .chain(self.device_library_status())
                    .chain(sync.typed.values().cloned())
                    .chain(std::iter::once(Event::SyncEpoch { epoch: generation }))
                    .chain(self.destination_controls_message())
                {
                    Self::push_to(
                        entry,
                        SharedEvent::new(self.reconcile_event(frame, &sync, snapshot_started)),
                    );
                }
                // Lock-free (no client/sync locks held): remember the
                // emitted native view so the ticker stays quiet until it
                // changes. A push landing between the sample above and
                // this record only delays one frame (views are cumulative
                // state, nothing is lost).
                self.record_native_view();
            }
            // Siblings learn about the join; the newcomer already has it.
            self.broadcast_except(self.clients_message(), &id);
            // A fresh page on a seeded-but-incomplete FX cache (boot with a
            // numbered rack) completes the params in the background instead of
            // waiting out the one-param-per-tick poller trickle.
            if self.hydrating.load(Ordering::Acquire) != 0 || !self.fx_cache_complete() {
                self.kick_fill();
            }
            return ClientHandle {
                id: id.clone(),
                engine: Arc::clone(self),
                events: ClientMailbox::new(rx, tx, rt, queued),
            };
        }
    }

    /// Queue a FRESH snapshot into an already-attached client's mailbox
    /// the resume path for adapters whose transport was suspended (mobile
    /// WebView backgrounded: the pushes stop, the world moves on). Same
    /// frame set an [`attach`](Engine::attach) snapshot carries, minus
    /// `you`, but with the FULL `caps` catalogue (attach ships the slim
    /// lane; a resumed UI already holds the catalogue and must never have
    /// it downgraded): `state`, `shows`, `clients`, `locks`, `caps`,
    /// `devices`, `link`, `info` (best-effort), `scenes`, `fx`, `routing`,
    /// `masters_snapshot`, then one `params` frame per cached channel,
    /// then the read-only `native_snapshots` frame, then (device mode
    /// only) the `local_library_status` frame.
    ///
    /// Never re-inserts the client, never touches locks/roles, and no-ops
    /// when the id is not attached. Cheap enough to be idempotent: adapters
    /// call it on every foreground transition so a suspended UI catches up
    /// in one batch instead of replaying frames queued while hidden.
    pub async fn push_snapshot(&self, client_id: &str) {
        let attached = self.clients.read().unwrap().contains_key(client_id);
        if !attached {
            return;
        }
        if self.active_drv().is_hardware() && !self.reads_suspended.load(Ordering::Acquire) {
            // Explicit native resync is a generation boundary, unlike
            // ordinary push delivery. Hydrate within the existing 5s budget;
            // unknowns remain absent if that budget is spent.
            let generation = self.invalidate_connection();
            self.param_cache.write().unwrap().clear();
            for row in self.fx_cache.write().unwrap().iter_mut() {
                row.params.clear();
            }
            self.hydrating.store(generation, Ordering::Release);
            if let Err(error) = self.active_drv().refresh_streams().await {
                eprintln!("engine: resume stream renewal failed: {error}");
            }
            self.hydrate_initial(generation).await;
            let _ =
                self.hydrating
                    .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            self.refresh_fx_cache().await;
        }
        // Build everything before touching the mailbox: same order as
        // `attach` (R1 prefix then R2), so a resumed client parses exactly
        // what a fresh one does.
        let generation = self.connection_gen.load(Ordering::Acquire);
        let snapshot_started = Instant::now();
        let state = self.state_message(None).await;
        let shows = self.shows_message();
        let roster = self.clients_message();
        let locks = self.locks_message();
        let caps = self.caps_message();
        let devices = self.devices_message();
        let link = self.link_message_cached();
        let info = self.info_message().await.ok();
        let scenes = self.scenes_message();
        let routing = self.routing_message().await;
        let masters = self.masters_snapshot().await;
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        let params = self.params_snapshot();
        let fx = self.fx_message_cached();
        // Read-only native slots ride after the param plane (same position
        // as `attach`), before the epoch barrier.
        let native = self.native_snapshots_message();
        for event in [state, shows, roster, locks, caps, devices, link]
            .into_iter()
            .chain(info)
            .chain([scenes, fx, routing])
            .chain(masters)
            .chain(params)
            .chain(std::iter::once(native))
            .chain(self.device_library_status())
            .chain(sync.typed.values().cloned())
            .chain(std::iter::once(Event::SyncEpoch { epoch: generation }))
            .chain(self.destination_controls_message())
        {
            self.unicast(
                client_id,
                self.reconcile_event(event, &sync, snapshot_started),
            );
        }
        drop(sync);
        self.record_native_view();
    }

    /// Replace a connected client's access policy (default: admin).
    ///
    /// Adapters use this to host restricted roles (e.g. musician) on one
    /// shared session; denied writes surface as unicast `error` events.
    /// Returns false when the client is unknown.
    pub fn set_client_policy(&self, id: &str, policy: Policy) -> bool {
        match self.clients.write().unwrap().get_mut(id) {
            Some(entry) => {
                entry.policy = policy;
                true
            }
            None => false,
        }
    }

    /// The client's current role (admin when unknown).
    pub fn client_role(&self, id: &str) -> Role {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .map(|e| e.policy.role)
            .unwrap_or(Role::Admin)
    }
}
