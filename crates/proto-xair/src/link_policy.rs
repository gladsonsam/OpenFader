use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;

pub(super) const IDS: [&str; 4] = ["preamp", "eq", "dyn", "fdrmute"];
pub(super) type Cache =
    std::sync::Arc<std::sync::Mutex<[(u64, Option<(std::time::Instant, bool)>); 4]>>;

pub(super) fn parse(msg: &OscMessage) -> Option<Vec<(usize, bool)>> {
    if msg.addr == "/config/linkcfg" {
        match msg.args.as_slice() {
            [OscType::Int(a @ 0..=1), OscType::Int(b @ 0..=1), OscType::Int(c @ 0..=1), OscType::Int(d @ 0..=1)] => {
                Some(vec![(0, *a == 1), (1, *b == 1), (2, *c == 1), (3, *d == 1)])
            }
            _ => None,
        }
    } else {
        let id = msg.addr.strip_prefix("/config/linkcfg/")?;
        let index = IDS.iter().position(|candidate| *candidate == id)?;
        match msg.args.as_slice() {
            [OscType::Int(value @ 0..=1)] => Some(vec![(index, *value == 1)]),
            _ => None,
        }
    }
}

pub(super) fn record(
    cache: &Cache,
    pending: &std::sync::atomic::AtomicU8,
    msg: &OscMessage,
    at: std::time::Instant,
    generation: &std::sync::atomic::AtomicU64,
    expected: u64,
) {
    if let Some(values) = parse(msg) {
        let mut state = cache.lock().unwrap();
        if generation.load(Ordering::Acquire) != expected {
            return;
        }
        for (index, value) in values {
            if pending.load(Ordering::Acquire) & (1 << index) != 0 {
                continue;
            }
            if state[index].1.is_none_or(|(_, old)| old != value) {
                state[index].0 = state[index].0.wrapping_add(1);
            }
            state[index].1 = Some((at, value));
        }
    }
}

impl XAirDriver {
    pub(super) fn refresh_link_policies(
        &self,
        session: std::sync::Arc<OscSession>,
        generation: u64,
    ) {
        let cache = self.link_policy_cache.clone();
        let current = self.connection_generation.clone();
        let pending = self.link_policy_pending.clone();
        let task = tokio::spawn(async move {
            for id in IDS {
                let index = IDS.iter().position(|candidate| *candidate == id).unwrap();
                let revision = cache.lock().unwrap()[index].0;
                if let Ok(msg) = session.query(&format!("/config/linkcfg/{id}")).await {
                    if let Some(values) = parse(&msg) {
                        let mut state = cache.lock().unwrap();
                        if current.load(Ordering::Acquire) == generation
                            && pending.load(Ordering::Acquire) & (1 << index) == 0
                            && state[index].0 == revision
                            && values.len() == 1
                            && values[0].0 == index
                        {
                            state[index].0 = state[index].0.wrapping_add(1);
                            state[index].1 = Some((std::time::Instant::now(), values[0].1));
                        }
                    }
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }

    pub(super) fn link_policy_controls(&self) -> Vec<DestinationControl> {
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let cache = self.link_policy_cache.lock().unwrap();
        if self.connection_generation.load(Ordering::Acquire) != generation {
            return vec![];
        }
        let topology = self.topology();
        let Some(members) = (0..self.strip_count())
            .map(|index| openfader_core::strip_key_for_index(&topology, index))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };
        let labels = ["Preamp", "EQ", "Dynamics", "Fader/Mute"];
        (0..4)
            .map(|i| DestinationControl {
                revision: Some(cache[i].0),
                generation: Some(generation),
                preset: false,
                members: members.clone(),
                target: "link_policies".into(),
                target_label: "Link policies".into(),
                target_name: None,
                send_slot: None,
                surface: "link_policy".into(),
                description: "Changes shared stereo-link behavior. Full mixer access is required."
                    .into(),
                id: IDS[i].into(),
                label: labels[i].into(),
                tap_tempo: false,
                gesture: None,
                choices: vec![
                    DestinationChoice {
                        value: 0.0,
                        label: "Off".into(),
                    },
                    DestinationChoice {
                        value: 1.0,
                        label: "On".into(),
                    },
                ],
                range: None,
                current: cache[i]
                    .1
                    .filter(|(at, _)| {
                        self.link_policy_pending.load(Ordering::Acquire) & (1 << i) == 0
                            && at.elapsed() <= std::time::Duration::from_secs(5)
                    })
                    .map(|(_, value)| DestinationValue {
                        value: u8::from(value) as f32,
                        verified: true,
                    }),
            })
            .collect()
    }
}

struct PolicyPending<'a> {
    driver: &'a XAirDriver,
    index: usize,
    generation: u64,
    confirmed: bool,
}

impl Drop for PolicyPending<'_> {
    fn drop(&mut self) {
        let mut cache = self.driver.link_policy_cache.lock().unwrap();
        if self.driver.connection_generation.load(Ordering::Acquire) != self.generation {
            return;
        }
        if !self.confirmed {
            cache[self.index].0 = cache[self.index].0.wrapping_add(1);
            cache[self.index].1 = None;
        }
        self.driver
            .link_policy_pending
            .fetch_and(!(1 << self.index), Ordering::AcqRel);
        drop(cache);
        self.driver.invalidate_policy_admission();
    }
}

impl XAirDriver {
    fn invalidate_policy_admission(&self) {
        *self.link_cache.lock().unwrap() = [None; 8];
        for cell in self.bus_link_cache.lock().unwrap().iter_mut() {
            cell.0 = cell.0.wrapping_add(1);
            cell.1 = None;
        }
    }

    /// Write one link policy only if the complete verified snapshot and caller's
    /// admission check still hold. The returned control has independent readback.
    pub async fn set_link_policy_checked(
        &self,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let index = IDS
            .iter()
            .position(|key| *key == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown link policy"))?;
        anyhow::ensure!(value == 0.0 || value == 1.0, "Invalid link policy value");
        let _lane = self.link_policy_write.lock().await;
        let _dispatch = self.bus_dispatch.write().await;
        let _preamp = self.preamp_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            check() && self.link_policy_controls() == snapshot && snapshot.len() == 4,
            "Link policies changed; reopen the control"
        );
        anyhow::ensure!(
            snapshot[index]
                .current
                .as_ref()
                .is_some_and(|v| v.verified && v.value == expected)
                && snapshot
                    .iter()
                    .all(|row| row.current.as_ref().is_some_and(|v| v.verified)),
            "Wait for complete link policy readback"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        for (i, row) in snapshot.iter().enumerate() {
            let reply = session
                .query_timeout(
                    &format!("/config/linkcfg/{}", IDS[i]),
                    std::time::Duration::from_millis(350),
                )
                .await?;
            anyhow::ensure!(
                check()
                    && self.connection_generation.load(Ordering::Acquire) == generation
                    && parse(&reply).is_some_and(|v| v.len() == 1
                        && v[0].0 == i
                        && row
                            .current
                            .as_ref()
                            .is_some_and(|r| r.value == u8::from(v[0].1) as f32)),
                "Link policies changed; reopen the control"
            );
        }
        anyhow::ensure!(
            self.link_policy_controls() == snapshot,
            "Link policies changed; reopen the control"
        );
        {
            let mut cache = self.link_policy_cache.lock().unwrap();
            anyhow::ensure!(
                self.connection_generation.load(Ordering::Acquire) == generation
                    && self.link_policy_pending.load(Ordering::Acquire) == 0
                    && cache
                        .iter()
                        .zip(snapshot)
                        .all(|(cell, row)| Some(cell.0) == row.revision
                            && cell.1.is_some_and(|(at, v)| at.elapsed()
                                <= std::time::Duration::from_secs(5)
                                && row
                                    .current
                                    .as_ref()
                                    .is_some_and(|r| r.value == u8::from(v) as f32))),
                "Link policies changed; reopen the control"
            );
            self.link_policy_pending
                .fetch_or(1 << index, Ordering::AcqRel);
            cache[index].0 = cache[index].0.wrapping_add(1);
            cache[index].1 = None;
        }
        let mut pending = PolicyPending {
            driver: self,
            index,
            generation,
            confirmed: false,
        };
        self.invalidate_policy_admission();
        let address = format!("/config/linkcfg/{id}");
        let guard = self.session().await?;
        anyhow::ensure!(
            guard
                .as_ref()
                .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session)),
            "Link policy session changed"
        );
        session
            .send_checked(&address, vec![OscType::Int(value as i32)], &|| {
                anyhow::ensure!(
                    check()
                        && self.connected.load(Ordering::Acquire)
                        && self.connection_generation.load(Ordering::Acquire) == generation,
                    "Link policy access or session changed"
                );
                Ok(())
            })
            .await?;
        drop(guard);
        for _ in 0..8 {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let reply = session
                .query_timeout(&address, std::time::Duration::from_millis(350))
                .await;
            anyhow::ensure!(
                check()
                    && self.connected.load(Ordering::Acquire)
                    && self.connection_generation.load(Ordering::Acquire) == generation,
                "Link policy result uncertain: access or session changed"
            );
            if reply
                .as_ref()
                .ok()
                .and_then(parse)
                .is_some_and(|v| v.len() == 1 && v[0] == (index, value == 1.0))
            {
                let mut cache = self.link_policy_cache.lock().unwrap();
                anyhow::ensure!(
                    self.connection_generation.load(Ordering::Acquire) == generation,
                    "Link policy session changed"
                );
                cache[index].0 = cache[index].0.wrapping_add(1);
                cache[index].1 = Some((std::time::Instant::now(), value == 1.0));
                pending.confirmed = true;
                drop(cache);
                drop(pending);
                return self
                    .link_policy_controls()
                    .into_iter()
                    .find(|row| {
                        row.id == id
                            && row
                                .current
                                .as_ref()
                                .is_some_and(|v| v.verified && v.value == value)
                    })
                    .ok_or_else(|| anyhow::anyhow!("Link policy result unavailable"));
            }
        }
        anyhow::bail!("Link policy command sent; independent readback unavailable")
    }
}
