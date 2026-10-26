//! Poll catalogue operations.

use super::*;

impl Engine {
    /// Full param-cache snapshot: one `params` event per channel that has
    /// any cached value (unicast to newly attached clients).
    pub(super) fn params_snapshot(&self) -> Vec<Event> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        let cache = self.param_cache.read().unwrap();
        let mut chs: Vec<u16> = cache.keys().copied().collect();
        chs.sort_unstable();
        chs.into_iter()
            .filter_map(|ch| {
                let values = cache.get(&ch)?;
                if values.is_empty() {
                    return None;
                }
                Some(Event::Params {
                    ch,
                    values: values
                        .iter()
                        .filter(|(k, _)| !k.starts_with('_'))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    key: openfader_core::strip_key_for_index(&topo, ch as usize),
                    // Snapshot of the verified-only cache: every emitted key
                    // is explicitly marked `verified`. Absent/unknown is
                    // NEVER verified, the field is optional only for
                    // backward compatibility with old frames.
                    authority: Some(
                        values
                            .keys()
                            .filter(|k| !k.starts_with('_'))
                            .map(|k| (k.clone(), "verified".to_string()))
                            .collect(),
                    ),
                })
            })
            .collect()
    }

    /// Round-robin poll list for one driver: every strip of every topology
    /// family crossed with the driver's supported catalogue ids, filtered
    /// by catalogue scopes (DCA strips get fader/mute/name/color only) and
    /// minus dropped pairs. Strip-major: every param of strip N precedes
    /// strip N+1, so one strip's whole set arrives as a block. Built on the
    /// cached static list (zero per-tick allocation in the poller).
    /// Test/diagnostic helper (the poller uses the static list directly).
    #[allow(dead_code)]
    pub(super) async fn poll_list(&self, driver: &str) -> Vec<(u16, String)> {
        let dropped = self.param_dropped.read().unwrap();
        self.static_poll_list(driver)
            .into_iter()
            .filter(|(_, id)| {
                !id.starts_with('_')
                    && *id != "rtnsw"
                    && !dropped.contains(&(driver.to_string(), id.to_string()))
            })
            .map(|(ch, id)| (ch, id.to_string()))
            .collect()
    }

    /// Cached static poll list: topology × capabilities × scopes, built
    /// once per driver. Drop-class filtering happens at drain time.
    pub(super) fn static_poll_list(&self, driver: &str) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_static.read().unwrap();
            if let Some(v) = cache.get(driver) {
                return v.clone();
            }
        }
        let topo = self.driver_topology(driver);
        let caps = self.driver_capabilities(driver);
        let reg = self.registry(driver);
        let mut out = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                for id in &caps {
                    let scoped = reg
                        .find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    out.push((s as u16, *id));
                }
                // Solo rides the poll set as its own id (never a catalogue
                // param, a solosw bank has no per-strip query address,
                // so it can never enter `caps`): strips carrying the
                // extra id re-read the bank via `get_solo`
                // (`read_param` routes it). Appended to the strip's own block, so the list
                // stays strip-major. This is the desk→app backstop for
                // solos the live ingest never saw (attach gaps, dropped
                // broadcasts); the ingest is the fast path.
                for id in self.drv(driver).essentials_extra_ids(fam.kind) {
                    out.push((s as u16, id));
                }
            }
        }
        for (ch, id) in self.drv(driver).static_extra_rows(&topo) {
            out.push((ch, id));
        }
        out.sort_by_key(|(ch, _)| *ch);
        self.poll_static
            .write()
            .unwrap()
            .insert(driver.to_string(), out.clone());
        out
    }

    /// Move `chs` to the front of the poll queue (the `focus` command,
    /// attributed to `writer`). Most-recent-first: pending entries for the
    /// focused strips are dropped (a new focus supersedes older ones, this
    /// also de-duplicates floods) and the fresh set is pushed at the FRONT
    /// in catalogue order, so it drains before everything queued before.
    /// The queue is capped (~4 sweeps, oldest dropped from the back). A
    /// detached client's pending focus is dropped with it (see
    /// [`Engine::remove_client`]).
    ///
    /// Drain order is core-first across the whole focused set: pass 1 queues
    /// `fader`+`mute` of every strip (a 16-strip mixer bank is fader-live
    /// after ~32 ticks ≈ 1.6 s at the 20 q/s budget instead of waiting out
    /// a full strip-major catalogue sweep), pass 2 `name`+`pan`, pass 3 the
    /// remaining scoped ids in catalogue order. A single-strip focus still
    /// drains as one unbroken block ahead of the background sweep.
    ///
    /// R2: the focused strips also become the visible set (capped at 16,
    /// most-recent-first) backing the poller's visible-group lane
    /// (EQ/dyn/sends of visible strips ahead of the background sweep).
    pub(super) fn request_focus(&self, writer: &str, chs: &[u16]) {
        let driver = self.active_driver();
        let nstrips = self.driver_strips(&driver);
        let valid: Vec<u16> = chs
            .iter()
            .copied()
            .filter(|c| (*c as usize) < nstrips)
            .collect();
        let dropped = self.param_dropped.read().unwrap();
        let ids: Vec<String> = self
            .driver_capabilities(&driver)
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .map(|s| s.to_string())
            .collect();
        drop(dropped);
        // Scope-filter per strip (mirrors poll_list): only ids the strip's
        // kind supports are queued.
        let topo = self.driver_topology(&driver);
        let reg = self.registry(&driver);
        let kind_of = |ch: u16| {
            topo.iter()
                .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                .map(|f| f.kind)
        };
        let mut q = self.focus_queue.write().unwrap();
        q.retain(|(d, c, _, _)| d != &driver || !chs.contains(c));
        // Core-first drain order (three push waves; each wave pushes its
        // strips back-to-front so the queue head ends up in `chs` order):
        // fader+mute for every strip, then name+pan, then the rest.
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            let scoped: Vec<&String> = ids
                .iter()
                .filter(|id| {
                    !is_core_id(id, true)
                        && !is_core_id(id, false)
                        && reg
                            .find(id)
                            .map(|def| params::supports_kind(def, kind))
                            .unwrap_or(true)
                })
                .collect();
            for id in scoped.iter().rev() {
                q.push_front((driver.clone(), ch, (*id).clone(), writer.to_string()));
            }
        }
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in ["pan", "balance", "name"].iter().rev() {
                if !ids.iter().any(|s| s == id) {
                    continue;
                }
                if !reg
                    .find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true)
                {
                    continue;
                }
                q.push_front((driver.clone(), ch, id.to_string(), writer.to_string()));
            }
        }
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in ["mute", "fader"].iter().rev() {
                if !ids.iter().any(|s| s == id) {
                    continue;
                }
                if !reg
                    .find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true)
                {
                    continue;
                }
                q.push_front((driver.clone(), ch, id.to_string(), writer.to_string()));
            }
        }
        let max = 4 * nstrips * ids.len().max(1);
        while q.len() > max {
            q.pop_back();
        }
        drop(q);
        // R2 visible set: most-recent focused strips first, capped at 16.
        // Re-arms the one-shot visible boost (one full sweep of the new
        // set's EQ/dyn/sends, then the background owns the wire again).
        if !valid.is_empty() {
            let fresh = {
                let vis = self.visible.read().unwrap().clone();
                let mut fresh: Vec<u16> = valid;
                fresh.extend(vis.iter().copied());
                fresh.dedup();
                fresh.truncate(16);
                fresh
            };
            *self.visible.write().unwrap() = fresh;
            self.prio_cursor.store(0, Ordering::Relaxed);
            self.prio_taken.store(0, Ordering::Relaxed);
            self.prio_armed.store(true, Ordering::Relaxed);
        }
    }

    /// Pop the next focus entry for `driver`, skipping stale entries and
    /// params dropped since they were queued.
    pub(super) fn pop_focus(&self, driver: &str) -> Option<(u16, String)> {
        let dropped = self.param_dropped.read().unwrap();
        let mut q = self.focus_queue.write().unwrap();
        while let Some((d, ch, param, _)) = q.pop_front() {
            if d != driver {
                continue;
            }
            if dropped.contains(&(d, param.clone())) {
                continue;
            }
            return Some((ch, param));
        }
        None
    }
}
