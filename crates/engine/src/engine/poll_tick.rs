//! Poll tick operations.

use super::*;

impl Engine {
    /// One pipelined poller tick (R2): up to `effective_inflight` driver
    /// queries per tick, strict lanes (focus > visible-group one-shot >
    /// essentials > background), results applied in batch order. At default
    /// inflight 1 this is exactly `Engine::poll_once`'s budget with the
    /// lanes interleaved; `Engine::poll_once` itself is untouched (exact
    /// legacy single-step for tests).
    ///
    /// Fairness: the essentials cursor round-robins without end, so a
    /// perpetual essentials lane would starve the background sweep forever.
    /// The background therefore keeps one slot every `max(2, n)` ticks, at
    /// xair/sim n=4 that is 20 q/s, exactly the pre-lane steady-state
    /// background rate, while the essentials lane takes the other ~60 q/s
    /// (≈2-3 s per sweep of the ~150 fader/mute/name/pan/color/solo pairs).
    /// At n=1 whole ticks alternate (lanes one tick, background the next).
    ///
    /// Public so integration tests and diagnostics can drive (and time)
    /// the poller deterministically; the background task calls this every
    /// `poll_interval`.
    pub async fn poll_tick(self: &Arc<Self>) {
        if self.hydrating.load(Ordering::Acquire) != 0
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return;
        }
        let generation = self.connection_gen.load(Ordering::Acquire);
        if Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
            self.poll_skips.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if self.routing_dirty.swap(false, Ordering::AcqRel) {
            self.broadcast(self.routing_message().await);
        }
        let driver = self.active_driver();
        let n = self.effective_inflight(&driver);
        if self.repair_dirty.swap(false, Ordering::AcqRel) {
            let list = self.static_poll_list(&driver);
            let mut queue = self.focus_queue.write().unwrap();
            for (ch, id) in &list {
                if !queue
                    .iter()
                    .any(|(d, s, p, _)| d == &driver && s == ch && p == id)
                {
                    queue.push_back((driver.clone(), *ch, id.to_string(), String::new()));
                }
            }
            eprintln!("engine: {driver} reconciliation queued {} keys; regular query budget, no snapshot reset", list.len());
        }
        let mut batch: Vec<(u16, String)> = Vec::with_capacity(n);
        let focus_limit = self.drv(&driver).poll_focus_limit(n);
        while batch.len() < focus_limit {
            match self.pop_focus(&driver) {
                Some(e) => batch.push(e),
                None => break,
            }
        }
        if batch.len() < n {
            // Lanes with guaranteed background progress: the background
            // keeps one slot every max(2, n) ticks; the visible one-shot
            // boost takes the next share, the perpetual essentials lane the
            // rest. At n=1 this alternates whole ticks (lanes, background,
            // …); at n=4 the background keeps 1 slot in 4 (20 q/s, the
            // pre-lane steady-state rate) while the lanes take ~60 q/s.
            // Without the reservation the perpetual essentials lane would
            // starve the background sweep forever (its cursor wraps without
            // end, like the visible lane's did).
            let remaining = n - batch.len();
            let t = self.lane_tick.fetch_add(1, Ordering::Relaxed);
            let bg_share = self.drv(&driver).poll_bg_share(n, t, remaining);
            let mut rest = remaining - bg_share;
            let vis = self.pop_visible_batch(&driver, self.drv(&driver).poll_visible_take(rest));
            rest -= vis.len();
            batch.extend(vis);
            batch.extend(self.pop_core_batch(&driver, rest));
        }
        if batch.len() < n {
            let list = self.static_poll_list(&driver);
            if !list.is_empty() {
                let dropped = self.param_dropped.read().unwrap();
                let mut skips = 0usize;
                while batch.len() < n && skips <= list.len() {
                    let i = self.poll_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
                    let (ch, id) = &list[i];
                    if dropped.contains(&(driver.clone(), id.to_string())) {
                        skips += 1;
                        continue;
                    }
                    skips = 0;
                    batch.push((*ch, id.to_string()));
                }
            }
        }
        if batch.is_empty() {
            return;
        }
        let t0 = Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for (ch, param) in batch {
            let engine = Arc::clone(self);
            let driver = driver.clone();
            set.spawn(async move {
                let started = Instant::now();
                let result = engine.read_param(&driver, StripId(ch), &param).await;
                if engine.connection_gen.load(Ordering::Acquire) != generation {
                    return;
                }
                match result {
                    Ok(pv) => {
                        engine.note_read_ok(&driver, ch, &param);
                        engine.apply_received(ch, &param, &pv, Instant::now(), Some(started));
                    }
                    Err(e) => {
                        engine.note_read_error(&driver, ch, &param, &e);
                    }
                }
            });
        }
        while set.join_next().await.is_some() {}
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        // Adaptive pipelining: EMA of the batch wall time.
        let dt_us = t0.elapsed().as_micros() as u64;
        let prev = self.rtt_ema_us.load(Ordering::Relaxed);
        let ema = if prev == 0 {
            dt_us
        } else {
            (prev * 3 + dt_us) / 4
        };
        self.rtt_ema_us.store(ema, Ordering::Relaxed);
        if self
            .fx_tick
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(FX_POLL_EVERY)
        {
            self.fx_refresh_one().await;
        }
    }
}
