//! Connection epoch transitions and staged initial state hydration.

use super::*;

type FillResult = anyhow::Result<ParamValue>;

/// One concurrent fill read's outcome: a finished query (ok or desk
/// error), a pair the desk already pushed while the fill was opening
/// (skip without cost), or an exhausted deadline (stop the fill).
enum FillRead {
    Done(
        (
            u16,
            &'static str,
            Instant,
            Result<FillResult, tokio::time::error::Elapsed>,
        ),
    ),
    Skipped,
    Stop,
}

impl Engine {
    /// Shared post-switch flow for `connect` / `connect_device` / `offline`:
    /// fast-first. Invalidate parameter caches and queue the
    /// paint-critical `state` + `caps` + cached `link` + `info` set
    /// immediately: caps needs no I/O at all, and state/info share one
    /// connect-burst budget (300 ms, not N x 2 s tails). The remainder
    /// (state + info refresh on timeout, full FX params, routing, masters)
    /// lands via the fill worker ([`Engine::deferred_fill`]), one broadcast
    /// per frame as it arrives. When an offline change list is pending, the
    /// `offline_diff` frame rides along and the push gate opens (nothing is
    /// pushed without `offline_push`). `entering_offline` keeps the
    /// just-armed offline session intact (otherwise a fresh connect resolves
    /// any pending diff).
    pub(super) async fn post_switch(
        &self,
        writer: &str,
        out: &mut Vec<Event>,
        entering_offline: bool,
    ) -> anyhow::Result<()> {
        self.param_cache.write().unwrap().clear();
        let driver = self.active_driver();
        self.param_dropped
            .write()
            .unwrap()
            .retain(|(d, _)| d != &driver);
        self.poll_timeouts
            .write()
            .unwrap()
            .retain(|(d, _), _| d != &driver);
        // Push-cache drivers need the same ordered publication as query
        // drivers: reading their initial cache is cheap, but the normal
        // background sweep would otherwise take minutes to expose DSP.
        if self.active_drv().needs_hydration() || self.active_drv().is_hardware() {
            self.hydrating.store(
                self.connection_gen.load(Ordering::Acquire),
                Ordering::Release,
            );
        }
        // connect() already seeded FX for this session. Do not repeat its
        // type queries ahead of channel levels and bus masters.
        self.note_write(writer);
        self.touch_holder_locks(writer);
        // Paint-critical frames first: caps + cached link need no I/O at
        // all (the UI flips to online on caps); state + info share one
        // burst budget off the command path.
        out.push(self.caps_message());
        out.push(self.link_message_cached());
        let burst = openfader_transport::session::CONNECT_BURST_TIMEOUT;
        let fast = tokio::time::timeout(burst, async {
            tokio::join!(self.state_message(Some(writer)), self.info_message())
        })
        .await;
        if let Ok((state, info)) = fast {
            out.push(state);
            // `info` is `Result`: silent desks stay silent, as before.
            if let Ok(info) = info {
                out.push(info);
            }
        }
        // Budget spent (slow venue wifi): the deferred fill re-emits
        // both, so no frame is ever lost, first paint just waits for
        // the fill instead of stalling the command on N x 2 s tails.
        // The read-only native slot frame is sync and needs no I/O, so it
        // always rides along (fresh status on every switch, never the
        // last desk's slots).
        out.push(self.native_snapshots_message());
        out.extend(self.destination_controls_message());
        self.record_native_view();
        if entering_offline {
            self.kick_fill();
            return Ok(());
        }
        // A fresh connect ends any offline session: pending changes raise
        // offline_diff and wait for offline_push (or offline_discard).
        let pending = {
            let mut off = self.offline.write().unwrap();
            off.active = false;
            if off.changes.is_empty() {
                off.pending_push = false;
                false
            } else {
                off.pending_push = true;
                true
            }
        };
        if pending {
            out.push(self.offline_diff_message());
        }
        self.kick_fill();
        Ok(())
    }

    /// Deferred remainder of [`Engine::post_switch`]: priority control fill
    /// (including masters), state/info, then FX, routing and assignments.
    /// Each phase streams as it lands. Idempotent (cache diffs keep repeats quiet);
    /// every section is best-effort so a silent desk never fails the fill.
    pub(super) async fn deferred_fill(self: &Arc<Self>, generation: u64) {
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return;
        }
        if self.hydrating.load(Ordering::Acquire) == generation {
            self.hydrate_initial(generation).await;
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            // The burst already served pending focus/visible entries.
            // Keep failures queued for normal retry; do not re-read every
            // successfully hydrated input before serving other banks.
            {
                let cache = self.param_cache.read().unwrap();
                self.focus_queue.write().unwrap().retain(|(_, ch, id, _)| {
                    !cache.get(ch).is_some_and(|values| values.contains_key(id))
                });
            }
            self.prio_armed.store(false, Ordering::Relaxed);
            let _ =
                self.hydrating
                    .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
        }
        let sampled = Instant::now();
        let state = self.state_message(None).await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        {
            let sync = self.sync_state.lock().unwrap();
            self.broadcast(self.reconcile_event(state, &sync, sampled));
        }
        if let Ok(info) = self.info_message().await {
            self.broadcast(info);
        }
        // Lower-priority rack/routing/assignment detail follows the controls;
        // the initial fill already streamed bus/subgroup/main master levels.
        self.refresh_fx_cache().await;
        self.broadcast(self.fx_message().await);
        self.broadcast(self.routing_message().await);
        let sampled = Instant::now();
        let masters = self.masters_snapshot().await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        let sync = self.sync_state.lock().unwrap();
        for frame in masters {
            self.broadcast(self.reconcile_event(frame, &sync, sampled));
        }
    }

    /// Stop old-session work before replacing a driver session.
    pub(super) fn invalidate_connection(&self) -> u64 {
        // Epoch publication and snapshot delivery share the same barrier.
        let mut sync = self.sync_state.lock().unwrap();
        let generation = self.connection_gen.fetch_add(1, Ordering::AcqRel) + 1;
        *sync = SyncState::default();
        self.read_failures.write().unwrap().clear();
        self.broadcast(Event::SyncEpoch { epoch: generation });
        drop(sync);
        self.connection_changed.notify_waiters();
        self.hydrating.store(0, Ordering::Release);
        self.focus_queue.write().unwrap().clear();
        generation
    }

    /// Publish only under the session barrier, so a late read cannot refill
    /// a frontend that has already cleared for a different connection.
    pub(super) fn publish_initial(&self, generation: u64, event: Event, sampled: Instant) -> bool {
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return false;
        }
        self.broadcast(self.reconcile_event(event, &sync, sampled));
        true
    }

    /// Bus/subgroup/main display pairs arrive before channel labels or DSP.
    /// Publish each master immediately rather than waiting for assignments,
    /// solo banks, FX, or the rest of the master's snapshot to finish.
    pub(super) async fn hydrate_master_levels(
        &self,
        driver: &str,
        generation: u64,
        deadline: Instant,
    ) {
        let drv = self.drv(driver);
        let sampled = Instant::now();
        if let Ok((fader, muted)) = tokio::time::timeout(
            deadline
                .saturating_duration_since(sampled)
                .min(drv.initial_read_timeout()),
            async {
                if drv.concurrent_initial_master_reads() {
                    let (fader, muted) = tokio::join!(drv.get_main_fader(), drv.get_main_mute());
                    (fader.ok(), muted.ok())
                } else {
                    (
                        drv.get_main_fader().await.ok(),
                        drv.get_main_mute().await.ok(),
                    )
                }
            },
        )
        .await
        {
            self.publish_initial(generation, Event::Main { fader, muted }, sampled);
        }
        if sampled.elapsed() >= Duration::from_millis(300) {
            eprintln!(
                "engine: {driver} slow initial main: {:?}",
                sampled.elapsed()
            );
        }
        for bus in 0..drv.bus_count() {
            let sampled = Instant::now();
            let read = async {
                Ok::<_, anyhow::Error>((
                    drv.get_bus_fader(bus).await?,
                    drv.get_bus_mute(bus).await?,
                ))
            };
            if let Ok(Ok((fader, muted))) = tokio::time::timeout(
                deadline
                    .saturating_duration_since(sampled)
                    .min(drv.initial_read_timeout()),
                read,
            )
            .await
            {
                if !self.publish_initial(
                    generation,
                    Event::Bus {
                        bus,
                        fader,
                        muted,
                        key: Some(openfader_core::strip_key(
                            "bus",
                            bus.saturating_add(1) as u16,
                        )),
                    },
                    sampled,
                ) {
                    return;
                }
            }
            if sampled.elapsed() >= Duration::from_millis(300) {
                eprintln!(
                    "engine: {driver} slow initial bus {bus}: {:?}",
                    sampled.elapsed()
                );
            }
            if Instant::now() >= deadline {
                return;
            }
        }
        for group in 0..drv.subgroup_count() {
            let sampled = Instant::now();
            let read = async {
                Ok::<_, anyhow::Error>((
                    drv.get_subgroup_fader(group).await?,
                    drv.get_subgroup_mute(group).await?,
                ))
            };
            if let Ok(Ok((fader, muted))) = tokio::time::timeout(
                deadline
                    .saturating_duration_since(sampled)
                    .min(drv.initial_read_timeout()),
                read,
            )
            .await
            {
                if !self.publish_initial(
                    generation,
                    Event::Subgroup {
                        group,
                        fader,
                        muted,
                        key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                    },
                    sampled,
                ) {
                    return;
                }
            }
            if Instant::now() >= deadline {
                return;
            }
        }
    }

    /// Bounded concurrent fill, field-major across the entire topology:
    /// levels -> masters -> mutes/pan -> names/look -> EQ -> remaining fields.
    /// Provider scopes/capabilities still decide which leaves can be read.
    /// Reads within a phase fly concurrently (up to the driver's poll
    /// inflight bound; the transport serializes same-address queries while
    /// distinct addresses proceed in parallel), so the wall time is ~reads
    /// / inflight RTTs instead of the sum. A five-second burst never blocks
    /// the command path; unfinished fields retain this order in the
    /// existing poller's queue, without a new timer.
    pub(super) async fn hydrate_initial(&self, generation: u64) {
        use futures::StreamExt;
        let driver = self.active_driver();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(5);
        let inflight = self.max_inflight(&driver).max(1);
        let mut list = self.static_poll_list(&driver);
        // Stable sorting preserves provider order within each channel/phase.
        list.sort_by_key(|(ch, id)| (initial_load_priority(id), *ch));
        let mut reads = 0usize;
        let mut skipped = 0usize;
        'fill: for phase in 0..=5 {
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            if Instant::now() >= deadline {
                break 'fill;
            }
            while Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
                tokio::time::sleep(Duration::from_millis(5)).await;
                if self.connection_gen.load(Ordering::Acquire) != generation {
                    return;
                }
                if Instant::now() >= deadline {
                    break 'fill;
                }
            }
            // Fetch solo through the regular bank lane, after DSP.
            let mut phase_items: Vec<(u16, &'static str)> = list
                .iter()
                .filter(|(_, id)| initial_load_priority(id) == phase)
                .filter(|(ch, id)| {
                    *id != "solo"
                        && !self
                            .param_cache
                            .read()
                            .unwrap()
                            .get(ch)
                            .is_some_and(|values| values.contains_key(*id))
                        && !self
                            .param_dropped
                            .read()
                            .unwrap()
                            .contains(&(driver.clone(), id.to_string()))
                })
                .map(|(ch, id)| (*ch, *id))
                .collect();
            // The static list can hold the same (strip, id) twice (catalogue
            // plane plus extra rows). The old sequential pass skipped the
            // second hit via its live-cache check; drop it up front so a
            // duplicate never costs a query.
            phase_items.sort();
            phase_items.dedup();
            eprintln!(
                "engine: {driver} fill phase {phase}: {} items at {:?}",
                phase_items.len(),
                start.elapsed()
            );
            let mut pending = Vec::with_capacity(phase_items.len());
            for (ch, id) in phase_items {
                let driver = driver.clone();
                pending.push(async move {
                    // Recheck at task start: desk pushes landing during the
                    // fill already hydrated this pair (the old sequential
                    // pass saw them via its inline check).
                    let fresh = self
                        .param_cache
                        .read()
                        .unwrap()
                        .get(&ch)
                        .is_some_and(|values| values.contains_key(id))
                        || self
                            .param_dropped
                            .read()
                            .unwrap()
                            .contains(&(driver.clone(), id.to_string()));
                    if fresh {
                        return FillRead::Skipped;
                    }
                    let started = Instant::now();
                    // Bound each initial read so silent leaves do not stall first paint.
                    // The background poller retries under the normal budget.
                    let budget = deadline
                        .saturating_duration_since(started)
                        .min(self.drv(&driver).initial_read_timeout());
                    if budget.is_zero() {
                        return FillRead::Stop;
                    }
                    let result =
                        tokio::time::timeout(budget, self.read_param(&driver, StripId(ch), id))
                            .await;
                    FillRead::Done((ch, id, started, result))
                });
            }
            let reads_stream = futures::stream::iter(pending).buffer_unordered(inflight);
            tokio::pin!(reads_stream);
            while let Some(read) = reads_stream.next().await {
                if self.connection_gen.load(Ordering::Acquire) != generation {
                    return;
                }
                let (ch, id, started, result) = match read {
                    FillRead::Done(item) => item,
                    FillRead::Skipped => {
                        skipped += 1;
                        continue;
                    }
                    FillRead::Stop => break 'fill,
                };
                if started.elapsed() >= Duration::from_millis(300) {
                    eprintln!(
                        "engine: {driver} slow initial read {ch}/{id}: {:?}",
                        started.elapsed()
                    );
                }
                match result {
                    Ok(Ok(value)) => {
                        self.note_read_ok(&driver, ch, id);
                        self.apply_received(ch, id, &value, Instant::now(), Some(started));
                    }
                    Ok(Err(e)) => {
                        self.note_read_error(&driver, ch, id, &e);
                    }
                    // Per-read timeout: skip it and keep filling; the
                    // background poller retries under its own budget.
                    Err(_) => {}
                }
                reads += 1;
                if Instant::now() >= deadline {
                    break 'fill;
                }
            }
            // Replay each completed phase from verified cache only. Individual
            // reads already stream; this also covers attach/initial-caps gaps.
            for event in self.params_snapshot() {
                if !self.publish_initial(generation, event, start) {
                    return;
                }
            }
            if phase == 0 {
                eprintln!(
                    "engine: {driver} initial channel levels ready in {:?}",
                    start.elapsed()
                );
                self.hydrate_master_levels(&driver, generation, deadline)
                    .await;
                eprintln!(
                    "engine: {driver} initial levels ready in {:?}",
                    start.elapsed()
                );
            }
        }
        // Do not strand unfilled EQ behind a strip-major background sweep.
        // Preserve explicit focus ahead of this continuation and deduplicate
        // against it. The regular poll budget/fairness/write priority applies.
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return;
        }
        let cache = self.param_cache.read().unwrap();
        let dropped = self.param_dropped.read().unwrap();
        let mut queue = self.focus_queue.write().unwrap();
        for (ch, id) in list {
            if id != "solo"
                && !cache.get(&ch).is_some_and(|v| v.contains_key(id))
                && !dropped.contains(&(driver.clone(), id.to_string()))
                && !queue
                    .iter()
                    .any(|(d, c, p, _)| d == &driver && *c == ch && p == id)
            {
                queue.push_back((driver.clone(), ch, id.to_string(), String::new()));
            }
        }
        drop(queue);
        drop(dropped);
        drop(cache);
        drop(sync);
        eprintln!(
            "engine: {driver} initial fill {:?} ({reads} reads, {skipped} pushed-skipped); remainder streams via poller",
            start.elapsed()
        );
    }

    /// Wake the fill worker for one [`Engine::deferred_fill`] pass
    /// (coalesced by generation, see [`Engine::ensure_tasks`]).
    pub(super) fn kick_fill(&self) {
        self.fill_connection_gen.store(
            self.connection_gen.load(Ordering::Acquire),
            Ordering::Release,
        );
        self.fill_gen.fetch_add(1, Ordering::AcqRel);
        self.fill_notify.notify_one();
    }
}
