//! Poll reads operations.

use super::*;

impl Engine {
    pub(super) fn note_read_ok(&self, driver: &str, ch: u16, param: &str) {
        self.read_failures
            .write()
            .unwrap()
            .remove(&(driver.to_string(), ch, param.to_string()));
    }

    pub(super) fn note_read_error(
        &self,
        driver: &str,
        ch: u16,
        param: &str,
        error: &anyhow::Error,
    ) {
        if !is_unsupported_error(error) {
            return;
        }
        let mut failures = self.read_failures.write().unwrap();
        let (strikes, retry) = failures
            .entry((driver.to_string(), ch, param.to_string()))
            .or_insert((0, Instant::now()));
        *strikes = strikes.saturating_add(1);
        if !is_timeout_error(error) || *strikes >= POLL_TIMEOUT_STRIKES {
            *retry = Instant::now() + Duration::from_secs(15);
            eprintln!(
                "engine: {driver} strip {ch} {param}: {error}; retry in 15s (capability retained)"
            );
        }
    }

    /// Read one catalogue param from the named driver.
    pub(super) async fn read_param(
        &self,
        driver: &str,
        ch: StripId,
        param: &str,
    ) -> anyhow::Result<ParamValue> {
        // Solo is a poll-set id, not a catalogue param (no `get_param`
        // arm serves it): drivers with solo state answer the bank read
        // directly; the rest stay honestly unsupported.
        if self
            .read_failures
            .read()
            .unwrap()
            .get(&(driver.to_string(), ch.0, param.to_string()))
            .is_some_and(|(_, retry)| *retry > Instant::now())
        {
            anyhow::bail!("read in scoped retry cooldown");
        }
        let drv = self.drv(driver);
        if param == "_mute_group" {
            return drv
                .get_mute_group_verified(ch.0 as u8)
                .await
                .map(ParamValue::Bool);
        }
        if let Some(group) = param.strip_prefix("_mg").and_then(|g| g.parse::<u8>().ok()) {
            return drv
                .get_mute_group_assign_verified(ch, group - 1)
                .await
                .map(ParamValue::Bool);
        }
        if param == "solo" {
            return drv.get_solo_verified(ch).await.map(ParamValue::Bool);
        }
        drv.get_param(ch, param).await
    }

    /// One poller tick: yield to any in-flight write first (MR1 write
    /// priority), then drain one focus entry, else read the next (strip,
    /// param) in round-robin order; broadcast a diff frame on change, or
    /// drop the param on unsupported/unknown/timeout reads. Drop-class
    /// pairs are skipped in place (cursor already advanced) so a dropout
    /// costs one tick, not a rebuild.
    ///
    /// Legacy single-step: the background task runs [`Engine::poll_tick`]
    /// (R2 pipelining); this stays as the exact one-query-per-call path
    /// for tests and diagnostics.
    #[allow(dead_code)]
    pub(super) async fn poll_once(&self) {
        if Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
            self.poll_skips.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let driver = self.active_driver();
        if let Some((ch, param)) = self.pop_focus(&driver) {
            self.poll_read(&driver, ch, &param).await;
            return;
        }
        let list = self.static_poll_list(&driver);
        if list.is_empty() {
            return;
        }
        let i = self.poll_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
        let (ch, param) = &list[i];
        if self
            .param_dropped
            .read()
            .unwrap()
            .contains(&(driver.clone(), param.to_string()))
        {
            return;
        }
        self.poll_read(&driver, *ch, param).await;
    }

    /// Read one (channel, param) and broadcast the diff frame on change.
    pub(super) async fn poll_read(&self, driver: &str, ch: u16, param: &str) {
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        match self.read_param(driver, StripId(ch), param).await {
            Ok(pv) => {
                self.note_read_ok(driver, ch, param);
                if self.connection_gen.load(Ordering::Acquire) == generation {
                    self.apply_received(ch, param, &pv, Instant::now(), Some(started));
                }
            }
            Err(e) => {
                self.note_read_error(driver, ch, param, &e);
            }
        }
    }

    /// Handle one poll-read error: unknown/unsupported params drop from the
    /// poll set at once (rebroadcasting `caps` once); QUERY TIMEOUTS drop
    /// only after [`POLL_TIMEOUT_STRIKES`] consecutive timeouts with no
    /// success in between, one lost datagram on venue wifi must not evict
    /// a live param (eviction also flips `caps.unsupported`, hiding the
    /// control until reconnect). Everything else keeps polling silently.
    /// Returns true when the param was dropped.
    #[cfg(test)]
    pub(super) async fn note_poll_error(
        &self,
        driver: &str,
        param: &str,
        e: &anyhow::Error,
    ) -> bool {
        if !is_unsupported_error(e) {
            return false;
        }
        if is_timeout_error(e) {
            let strikes = {
                let mut pending = self.poll_timeouts.write().unwrap();
                let c = pending
                    .entry((driver.to_string(), param.to_string()))
                    .or_insert(0);
                *c = c.saturating_add(1);
                *c
            };
            if strikes < POLL_TIMEOUT_STRIKES {
                return false;
            }
            self.poll_timeouts
                .write()
                .unwrap()
                .remove(&(driver.to_string(), param.to_string()));
        }
        let fresh = {
            let mut dropped = self.param_dropped.write().unwrap();
            dropped.insert((driver.to_string(), param.to_string()))
        };
        if fresh {
            self.broadcast(self.caps_message());
        }
        fresh
    }

    /// Clear consecutive-timeout strikes after a successful read: a desk
    /// that answers again is alive and must never be evicted.
    #[cfg(test)]
    pub(super) fn note_poll_ok(&self, driver: &str, param: &str) {
        self.poll_timeouts
            .write()
            .unwrap()
            .remove(&(driver.to_string(), param.to_string()));
    }
}

/// Drop-class poll errors: the param has no wire address on this driver
/// (unknown/unsupported) or the desk never answers (timeouts). Everything
/// else, empty sim slots (`no value yet`), unpushed dl32s cache
/// (`no value pushed yet`), own-write-pending dl32s words with no desk
/// proof (`no desk-confirmed value yet`), `not connected`, transient
/// I/O, keeps polling.
pub(super) fn is_unsupported_error(e: &anyhow::Error) -> bool {
    let m = format!("{e:#}");
    m.contains("not supported") || m.contains("unknown param") || m.contains("timed out")
}

/// Timeout-class poll errors (a subset of [`is_unsupported_error`]): the
/// desk never answered. Transient on wifi, [`Engine::note_poll_error`]
/// evicts only after [`POLL_TIMEOUT_STRIKES`] in a row.
pub(super) fn is_timeout_error(e: &anyhow::Error) -> bool {
    format!("{e:#}").contains("timed out")
}

/// Connection-fill priority only; steady-state polling retains its lanes.
pub(super) fn initial_load_priority(id: &str) -> u8 {
    match id {
        "fader" => 0,
        "mute" | "pan" | "balance" => 1,
        "name" => 2,
        "icon" | "color" => 3,
        id if id.starts_with("eq") || id.starts_with("hpf_") => 4,
        _ => 5,
    }
}

/// R2 essentials-lane ids: strip basics + pan + look. Everything deeper
/// rides focus (visible strips) or the background sweep, see
/// [`Engine::core_poll_list`].
pub(super) fn is_essentials_id(id: &str) -> bool {
    matches!(id, "fader" | "mute" | "name" | "pan" | "balance" | "color")
}

pub(super) fn param_value_json(pv: &ParamValue) -> Value {
    match pv {
        ParamValue::Float(v) => json!(v),
        ParamValue::Bool(b) => json!(b),
        ParamValue::Text(s) => json!(s),
    }
}
