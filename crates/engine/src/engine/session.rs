//! Session operations.

use super::*;

impl Engine {
    pub fn active_driver(&self) -> String {
        self.active.read().unwrap().clone()
    }

    /// Erased driver for a registered name; external names must pass `known` first.
    pub(super) fn drv(&self, name: &str) -> Arc<dyn DynDriver> {
        self.drivers
            .get(name)
            .expect("driver names are validated against the registry before dispatch")
            .driver
            .clone()
    }

    /// Erased driver behind the current selection.
    pub(super) fn active_drv(&self) -> Arc<dyn DynDriver> {
        let active = self.active_driver();
        self.drv(&active)
    }

    /// Sorted registry keys for `unknown driver` diagnostics.
    pub(super) fn expected_drivers(&self) -> String {
        driver_names(&self.drivers).join("|")
    }

    pub(super) fn known(&self, driver: &str) -> bool {
        self.drivers.contains_key(driver)
    }

    pub(super) fn default_addr(&self, driver: &str) -> &str {
        self.drivers
            .get(driver)
            .expect("connection driver is validated before resolving its default address")
            .default_addr
    }

    /// Select `driver` and connect it. Failures leave the old driver
    /// active; callers (adapters) stay up so a client can retry.
    pub async fn connect(&self, driver: &str, addr: Option<&str>) -> anyhow::Result<()> {
        if !self.known(driver) {
            anyhow::bail!(
                "unknown driver {driver:?}; expected {}",
                self.expected_drivers()
            );
        }
        let target = addr
            .map(str::to_string)
            .unwrap_or_else(|| self.default_addr(driver).to_string());
        // Restoration is best-effort if the desk vanished; it must not
        // prevent disconnecting or selecting another driver.
        let _ = self.active_drv().set_spectrum_target("", None).await;
        let drv = self.drv(driver);
        let was_suspended = self.reads_suspended.swap(true, Ordering::AcqRel);
        let generation = self.invalidate_connection();
        if drv.needs_hydration() {
            self.hydrating.store(generation, Ordering::Release);
        }
        if let Err(e) = drv.connect(&target).await {
            self.hydrating.store(0, Ordering::Release);
            self.reads_suspended.store(was_suspended, Ordering::Release);
            // Updates suppressed during verification belong to a live old
            // session. Repair it instead of silently losing that interval.
            self.repair_dirty.store(true, Ordering::Release);
            // The epoch already went out (clients cleared on it) while the
            // previous driver/session is retained: refill it so clients do
            // not strand empty.
            self.replay_retained_session(generation).await;
            return Err(e);
        }
        let previous = self.active_driver();
        if previous != driver {
            let _ = self.drv(&previous).disconnect().await;
        }
        // Remember the desk address only on success: a failed connect must
        // not plant an unreachable address that every later scan unicast-
        // probes (slow scans, phantom rows). Hardware scans verify the
        // remembered addresses (broadcast discovery never crosses subnets).
        self.remember_desk(driver, &target);
        *self.active.write().unwrap() = driver.to_string();
        self.param_cache.write().unwrap().clear();
        self.fx_cache.write().unwrap().clear();
        self.reads_suspended.store(false, Ordering::Release);
        // The FX cache mirrors the active driver (sim rows / xair
        // placeholders); refresh it on every switch, including boot.
        // Seeding performs no hardware queries: actual FX types/params
        // follow channel levels, masters, mutes, names and EQ in the fill.
        self.seed_fx_cache().await;
        // Hardware meter streams need an explicit subscribe per connection
        // generation (sim synthesizes its own levels via `poll_meters`).
        // Best-effort: a failed subscribe leaves the ticker on silence,
        // never fails the connect. Every successful hardware connect
        // resubscribes: the driver drops its old session (clearing
        // `meter_latest`) and its old drain/renewal tasks exit on the
        // generation bump, so skipping the resubscribe here would leave
        // the ticker on silence forever (reconnect-same-driver,
        // disconnect-reconnect, xair->sim->xair all hit this).
        if drv.wants_meter_subscribe() {
            // Drop the previous generation's guard before resubscribing so
            // a failed subscribe cannot leave a stale receiver behind; the
            // next connect retries from clean state.
            *self.meter_guard.write().unwrap() = None;
            *self.meter_driver.write().unwrap() = None;
            match drv.subscribe_meters().await {
                Ok(rx) => {
                    *self.meter_guard.write().unwrap() = Some(rx);
                    *self.meter_driver.write().unwrap() = Some(driver.to_string());
                }
                Err(e) => {
                    eprintln!(
                        "engine: {driver} meter subscribe failed (meters stay silent): {e:#}"
                    );
                }
            }
        } else {
            // Leaving a hardware meter subscription: drop the guard so the
            // old broadcast receiver is not held past the switch. The
            // driver's own renewal/drain tasks are generation-gated and
            // exit on the next hardware connect (no accumulation).
            *self.meter_guard.write().unwrap() = None;
            *self.meter_driver.write().unwrap() = None;
        }
        Ok(())
    }

    /// Replay the retained session after a failed [`Engine::connect`]: the
    /// generation was already invalidated (clients cleared on its `SyncEpoch`)
    /// but the previous driver stays active with its caches intact. Re-emit
    /// its `caps` + cached `link` + fresh `state` + `masters` + cached
    /// `params` so clients refill instead of stranding empty. Generation-
    /// gated like [`Engine::deferred_fill`]: a racing switch wins and this
    /// replay stays silent; live reads are best-effort (a silent desk drops
    /// frames, never the caller's error).
    pub(super) async fn replay_retained_session(&self, generation: u64) {
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        let sampled = Instant::now();
        let state = self.state_message(None).await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        {
            let sync = self.sync_state.lock().unwrap();
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            self.broadcast(self.reconcile_event(self.caps_message(), &sync, sampled));
            self.broadcast(self.reconcile_event(self.link_message_cached(), &sync, sampled));
            self.broadcast(self.reconcile_event(state, &sync, sampled));
        }
        let masters = self.masters_snapshot().await;
        let info = self.info_message().await.ok();
        let routing = self.routing_message().await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        {
            let sync = self.sync_state.lock().unwrap();
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            for frame in info
                .into_iter()
                .chain([
                    self.fx_message_cached(),
                    routing,
                    self.scenes_message(),
                    self.native_snapshots_message(),
                ])
                .chain(self.destination_controls_message())
                .chain(masters)
            {
                self.broadcast(self.reconcile_event(frame, &sync, sampled));
            }
            for frame in self.params_snapshot() {
                self.broadcast(self.reconcile_event(frame, &sync, sampled));
            }
            drop(sync);
            self.record_native_view();
        }
    }

    /// Remember a verified desk address under `driver` and persist the map
    /// (best-effort). Only successful connects/probes reach here, so a scan
    /// never unicast-probes a dead host it invented. Sim addresses are not
    /// desks (a scan skips them anyway), so they never touch `desks.json`.
    /// Holds the `desk_addrs` write lock across the synchronous atomic
    /// persist (never across `.await`) so a racing `forget_saved_mixers`
    /// clear cannot interleave between the map insert and the disk write;
    /// failures only log and never fail the connect/probe.
    pub(super) fn remember_desk(&self, driver: &str, addr: &str) {
        if !self.drv(driver).is_hardware() {
            return;
        }
        let mut addrs = self.desk_addrs.write().unwrap();
        addrs.insert(driver.to_string(), addr.to_string());
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        // Best-effort atomic persist while holding the write lock (never
        // across `.await`): `persist_desks` only logs on failure, so a
        // read-only data dir never fails the connect/probe.
        persist_desks(Some(dir.as_path()), &addrs);
    }

    /// Pure-local forget of every saved mixer address: clears the in-memory
    /// remembered desk map and durably persists the empty map. No driver,
    /// query, connect, disconnect, probe, scan or native-view work; no
    /// library revision, health, generation, caps or meter changes. Sync
    /// (no `.await` inside, no `std` lock held across `.await`) so tests
    /// and the `forget_saved_mixers` command share one path.
    pub fn forget_saved_mixers(&self, request_id: String) -> Event {
        const OP: &str = "forget_saved_mixers";
        let ok = |request_id: String| Event::DeviceOperationComplete {
            request_id,
            operation: OP.to_string(),
            results: Vec::new(),
            error: None,
        };
        let err = |request_id: String, message: String| Event::DeviceOperationComplete {
            request_id,
            operation: OP.to_string(),
            results: Vec::new(),
            error: Some(message),
        };
        let Some(dir) = self.data_dir.clone() else {
            self.desk_addrs.write().unwrap().clear();
            return ok(request_id);
        };
        let mut addrs = self.desk_addrs.write().unwrap();
        let empty: HashMap<String, String> = HashMap::new();
        match write_desks_atomic(&dir, &empty) {
            Ok(DesksWriteOutcome::Committed) => {
                addrs.clear();
                drop(addrs);
                ok(request_id)
            }
            Ok(DesksWriteOutcome::Uncertain(message)) => {
                // Rename landed but the directory sync did not prove
                // durability: memory matches the (possibly unproven) disk.
                addrs.clear();
                drop(addrs);
                err(request_id, message)
            }
            Err(message) => err(request_id, message),
        }
    }
}
