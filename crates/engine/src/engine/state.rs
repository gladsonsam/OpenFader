//! State operations.

use super::*;

impl Engine {
    /// Full `state` event for the active driver, optionally tagged with the
    /// mutation writer (`by` is `None` for snapshots).
    pub(super) async fn state_message(&self, by: Option<&str>) -> Event {
        let name = self.active_driver();
        let strips = snapshot_strips(&*self.drv(&name)).await;
        let main_fader = self.active_main_fader().await;
        Event::State {
            output_meter_targets: self.drv(&name).output_meter_targets(),
            driver: name,
            strips,
            main_fader,
            by: by.map(str::to_string),
        }
    }

    /// Collect the writer-tagged `state` broadcast for `out` (flushed after
    /// the unicast reply, mirroring the relay socket order).
    pub(super) async fn push_state_with_by(&self, by: &str, out: &mut Vec<Event>) {
        if self.active_drv().mutation_state_from_observations() {
            out.push(self.observed_state_message(by));
        } else {
            out.push(self.state_message(Some(by)).await);
        }
    }

    /// Project confirmed values only: refresh_param evicts unconfirmed writes.
    /// An incomplete cache emits no legacy rows; descriptor-driven clients
    /// preserve topology and receive individual observed parameter events.
    fn observed_state_message(&self, by: &str) -> Event {
        let name = self.active_driver();
        let driver = self.drv(&name);
        let topology = driver.topology();
        let cache = self.param_cache.read().unwrap();
        let strips = topology
            .iter()
            .flat_map(|family| family.first..family.first + family.count)
            .map(|index| {
                let ch = u16::try_from(index).ok()?;
                let values = cache.get(&ch)?;
                let fader = values.get("fader")?.as_f64()? as f32;
                if !fader.is_finite() {
                    return None;
                }
                Some(crate::protocol::StateStrip {
                    ch,
                    key: openfader_core::strip_key_for_index(&topology, index),
                    name: values.get("name")?.as_str()?.to_owned(),
                    fader,
                    muted: values.get("mute")?.as_bool()?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .filter(|strips| strips.len() == driver.strip_count())
            .unwrap_or_default();
        let main = topology
            .iter()
            .find(|family| family.kind == openfader_core::StripKind::Main);
        let main_fader = main
            .and_then(|family| u16::try_from(family.first).ok())
            .and_then(|ch| cache.get(&ch))
            .and_then(|values| values.get("fader"))
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| value.is_finite());
        Event::State {
            output_meter_targets: driver.output_meter_targets(),
            driver: name,
            strips,
            main_fader,
            by: Some(by.to_owned()),
        }
    }

    /// Write-priority decision (MR1): the poller skips its tick while any
    /// command is inside [`apply`](Engine::apply) (`writes_in_flight > 0`).
    /// Pure function so the latency test can assert the rule directly.
    pub(super) fn poller_yields(writes_in_flight: u64) -> bool {
        writes_in_flight > 0
    }

    /// Monotonic engine clock in ms since [`Engine::new`] (source of the
    /// `t_ms` field in binary meter frames). Wraps at ~49.7 days; clients
    /// must treat it as monotonic-per-connection, not wall time.
    pub fn now_ms(&self) -> u32 {
        self.boot.elapsed().as_millis() as u32
    }

    /// Poller ticks skipped while a write was in flight (diagnostic for
    /// the MR1 latency test: proves the yield path executes).
    pub fn poll_skips(&self) -> u64 {
        self.poll_skips.load(Ordering::Relaxed)
    }

    /// This client's binary-meter preference (set by [`Cmd::Hello`]).
    /// Adapters read it after every inbound command to pick the `Meters`/`Gr`
    /// encoding; unknown clients read `false` (JSON).
    pub fn client_binary_meters(&self, id: &str) -> bool {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .is_some_and(|e| e.binary_meters)
    }

    /// Detach a client by id (adapter path: the inbound or sender task owns
    /// no [`ClientHandle`]). Idempotent, like [`ClientHandle::detach`].
    pub async fn detach_client(&self, id: &str) {
        self.remove_client(id);
    }
}

pub(super) async fn snapshot_strips(d: &dyn DynDriver) -> Vec<crate::protocol::StateStrip> {
    let topo = d.topology();
    match d.snapshot().await {
        Ok(snap) => snap
            .strips
            .iter()
            .map(|s| crate::protocol::StateStrip {
                ch: s.id.0,
                key: openfader_core::strip_key_for_index(&topo, s.id.0 as usize),
                name: s.name.clone(),
                fader: s.fader,
                muted: s.mute,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}
