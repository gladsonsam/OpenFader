//! Post-write readback, cache eviction, and driver snapshots.

use super::*;

impl Engine {
    /// Re-read one param after a client write, caching the confirmed value.
    /// Returns the confirmed JSON value (`None` when the re-read fails, in
    /// which case the caller falls back to echoing the written value as
    /// `accepted`, see the `authority` field on [`Event::Param`]).
    ///
    /// Provenance firewall (R2 preset-truth contract): providers whose
    /// `get_param` cannot see past their own writes (see
    /// [`DynDriver::readback_confirms_writes`], DL32S reads the same push
    /// cache its writes populate, and the desk never echoes own writes)
    /// short-circuit to `None` WITHOUT reading. Their cache would agree
    /// with every write by construction, so treating that "agreement" as
    /// confirmation would launder accepted-but-unverified writes into
    /// verified state (and let preset recall count desk clamps/failures as
    /// genuine). The engine `param_cache` therefore holds only
    /// desk-evidenced values: poll reads, desk pushes, and re-reads from
    /// providers with genuine readback. Write fallbacks never touch it.
    ///
    /// The desk applies writes with a delay of tens of ms: an immediate
    /// re-read usually returns the PRE-write value (owner-rig measured on
    /// XR18 FW 1.20, every clip of `fader` echo read back the previous
    /// value). A re-read that disagrees with what was just written is
    /// therefore treated as that stale pre-image: it is NOT cached (it
    /// would regress the broadcast-confirmed value) and the caller echoes
    /// the written value instead; the desk's own change broadcast confirms
    /// within ~100 ms. Floats agree within the desk's ~10-bit quantization
    /// step; bools/text must match exactly.
    ///
    /// Superseded-entry eviction (R2 preset-truth contract): every `None`
    /// path evicts the pre-write `param_cache` entry (unless a genuinely
    /// newer desk read landed after this write started, see
    /// [`evict_param_unless_fresh`](Self::evict_param_unless_fresh)), so
    /// `params_snapshot`/attach can never re-emit the stale pre-write value
    /// as verified proof after the frontend invalidated it. A genuine desk
    /// push re-inserts proof via `apply_received`.
    pub(super) async fn refresh_param(
        &self,
        ch: StripId,
        param: &str,
        written: &Value,
    ) -> Option<Value> {
        let write_started = Instant::now();
        if !self.active_drv().readback_confirms_writes() {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        let driver = self.active_driver();
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let pv = match self.read_param(&driver, ch, param).await {
            Ok(pv) => pv,
            Err(_) => {
                self.evict_param_unless_fresh(ch, param, write_started);
                return None;
            }
        };
        if generation != self.connection_gen.load(Ordering::Acquire) {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        let v = param_value_json(&pv);
        let equivalent = json_to_param_value(written)
            .ok()
            .and_then(|written| {
                self.drv(&driver)
                    .param_readback_equivalent(param, &pv, &written)
            })
            .unwrap_or_else(|| refresh_agrees(&v, written));
        if !equivalent {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        self.apply_received(ch.0, param, &pv, Instant::now(), Some(started));
        self.param_cache
            .read()
            .unwrap()
            .get(&ch.0)
            .and_then(|values| values.get(param))
            .cloned()
    }

    /// Evict one superseded `param_cache` entry after an accepted
    /// (unverified) write, unless a genuinely newer desk read already
    /// re-proved it: `sync.received` timestamps newer than this write's
    /// start mean a poll/push landed after the write began, and that proof
    /// must survive. Hold the sync guard through cache deletion, matching
    /// the sync-then-cache lock order of `apply_received`/`reconcile_event`:
    /// a push that lands before the evict check is kept, one that lands
    /// after re-inserts via `apply_received`.
    pub(super) fn evict_param_unless_fresh(
        &self,
        ch: StripId,
        param: &str,
        write_started: Instant,
    ) {
        let sync = self.sync_state.lock().unwrap();
        let fresh = sync
            .received
            .get(&(ch.0, param.to_string()))
            .is_some_and(|(at, _)| *at > write_started);
        if fresh {
            return;
        }
        if let Ok(mut cache) = self.param_cache.write() {
            if let Some(values) = cache.get_mut(&ch.0) {
                values.remove(param);
                if values.is_empty() {
                    cache.remove(&ch.0);
                }
            }
        }
    }

    /// Capture read (R2 preset-truth contract): desk-verified live read
    /// first; on a miss, honest continuity for the display-basic leaves
    /// only (`fader`/`mute`/`name` via the inclusive display getters, which
    /// include own writes without claiming proof). Every other param returns
    /// `None` on a miss, a capture omits unproven values rather than
    /// laundering the stale pre-write cache entry (which the accepted write
    /// already evicted) as current desk state. Continuity values recorded
    /// here carry no provenance in the stored scene; the recall path re-emits
    /// hardware echoes as `accepted`.
    pub(super) async fn capture_read_param(
        &self,
        driver: &str,
        ch: StripId,
        param: &str,
    ) -> Option<ParamValue> {
        if let Ok(pv) = self.read_param(driver, ch, param).await {
            return Some(pv);
        }
        let drv = self.drv(driver);
        match param {
            "fader" => drv.get_fader(ch).await.ok().map(ParamValue::Float),
            "mute" => drv.get_mute(ch).await.ok().map(ParamValue::Bool),
            "name" => drv.get_name(ch).await.ok().map(ParamValue::Text),
            _ => None,
        }
    }

    /// Pinned snapshot (r58 P2(2)): the bulk read and the param-plane
    /// fallback both bind to `pin_driver` + `generation`, with a fail-fast
    /// check after the bulk read and per strip in the fallback. The commit
    /// barrier re-validates before publish in any case.
    pub(super) async fn active_snapshot_on(
        &self,
        pin_driver: &str,
        generation: u64,
    ) -> anyhow::Result<openfader_core::MixerSnapshot> {
        let driver = pin_driver.to_string();
        match self.drv(&driver).snapshot().await {
            Ok(snapshot) => {
                anyhow::ensure!(
                    self.connection_gen.load(Ordering::Acquire) == generation,
                    "desk changed during scene capture"
                );
                anyhow::ensure!(
                    !self.reads_suspended.load(Ordering::Acquire),
                    "desk session changed during scene capture (reads suspended)"
                );
                Ok(snapshot)
            }
            Err(first) => {
                if !self.drv(&driver).param_plane_snapshot() {
                    return Err(first);
                }
                let mut snapshot = openfader_core::MixerSnapshot::default();
                for family in self.driver_topology(&driver) {
                    for index in family.first..family.first + family.count {
                        let id = StripId(index as u16);
                        let (fader, mute, name) = tokio::join!(
                            self.capture_read_param(&driver, id, "fader"),
                            self.capture_read_param(&driver, id, "mute"),
                            self.capture_read_param(&driver, id, "name"),
                        );
                        let (
                            Some(ParamValue::Float(fader)),
                            Some(ParamValue::Bool(mute)),
                            Some(ParamValue::Text(name)),
                        ) = (fader, mute, name)
                        else {
                            anyhow::bail!("{driver}: no snapshot values for strip {index}");
                        };
                        anyhow::ensure!(
                            self.connection_gen.load(Ordering::Acquire) == generation,
                            "desk changed during scene capture"
                        );
                        anyhow::ensure!(
                            !self.reads_suspended.load(Ordering::Acquire),
                            "desk session changed during scene capture (reads suspended)"
                        );
                        snapshot.strips.push(openfader_core::ChannelStrip {
                            id,
                            name,
                            fader,
                            mute,
                            pan: 0.5,
                        });
                    }
                }
                Ok(snapshot)
            }
        }
    }

    /// Main fader of the active driver, `None` when the driver has none.
    pub(super) async fn active_main_fader(&self) -> Option<f32> {
        let driver = self.active_driver();
        self.main_fader_on(&driver).await
    }

    /// Pinned main-fader read (r58 P2(2)): the capture's desk, not
    /// whatever is active when the await lands.
    pub(super) async fn main_fader_on(&self, driver: &str) -> Option<f32> {
        self.drv(driver).get_main_fader().await.ok()
    }
}

/// JSON engineering value back to a driver [`ParamValue`].
pub(super) fn json_to_param_value(v: &Value) -> anyhow::Result<ParamValue> {
    match v {
        Value::Number(n) => Ok(ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32)),
        Value::Bool(b) => Ok(ParamValue::Bool(*b)),
        Value::String(s) => Ok(ParamValue::Text(s.clone())),
        _ => anyhow::bail!("scene value must be number|bool|string, got {v}"),
    }
}
