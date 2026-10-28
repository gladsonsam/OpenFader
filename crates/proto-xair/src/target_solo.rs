//! XR18 Main/DCA monitoring switches, with verified per-target admission.
use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
struct Row {
    revision: u64,
    value: Option<bool>,
    observed: Option<Instant>,
    floor: Option<Instant>,
    pending: bool,
}
impl Row {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.value = None;
        self.observed = None;
        self.floor = Some(Instant::now());
    }
}
#[derive(Default)]
pub(super) struct Cache {
    pub generation: u64,
    rows: [Row; 5],
}
fn target_index(target: &str) -> Option<usize> {
    match target {
        "main" => Some(0),
        "vca/01" => Some(1),
        "vca/02" => Some(2),
        "vca/03" => Some(3),
        "vca/04" => Some(4),
        _ => None,
    }
}
fn value(msg: &OscMessage, index: usize) -> anyhow::Result<bool> {
    anyhow::ensure!(msg.addr == addrs::SOLO_SW, "Unexpected solo reply");
    super::monitor::solo_value(msg.args.get(49 + index), 49 + index)
}
pub(super) fn invalidate(cache: &std::sync::Mutex<Cache>, generation: u64) {
    let mut c = cache.lock().unwrap();
    if c.generation == generation {
        for row in &mut c.rows {
            row.invalidate();
        }
    }
}
pub(super) fn record(
    cache: &std::sync::Mutex<Cache>,
    msg: &OscMessage,
    at: Instant,
    generation: u64,
) {
    let leaf = msg
        .addr
        .strip_prefix("/-stat/solosw/")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| (50..=54).contains(i));
    if msg.addr != addrs::SOLO_SW && leaf.is_none() {
        return;
    }
    let mut c = cache.lock().unwrap();
    if c.generation != generation {
        return;
    }
    for (i, row) in c.rows.iter_mut().enumerate() {
        if leaf.is_some_and(|n| n != i + 50)
            || row.pending
            || row.floor.is_some_and(|floor| at < floor)
            || row.observed.is_some_and(|prior| at < prior)
        {
            continue;
        }
        let decoded = if leaf.is_some() {
            super::monitor::solo_value(msg.args.first(), i + 49)
        } else {
            value(msg, i)
        };
        match decoded {
            Ok(v) => {
                if row.value != Some(v) {
                    row.revision = row.revision.wrapping_add(1);
                }
                row.value = Some(v);
                row.observed = Some(at);
            }
            Err(_) => row.invalidate(),
        }
    }
}
struct Pending<'a> {
    driver: &'a XAirDriver,
    generation: u64,
    index: usize,
    confirmed: bool,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut c = self.driver.target_solo.lock().unwrap();
        if c.generation == self.generation {
            let row = &mut c.rows[self.index];
            if !self.confirmed {
                row.invalidate();
            }
            row.pending = false;
        }
    }
}
impl XAirDriver {
    pub(super) fn refresh_target_solo(&self, session: std::sync::Arc<OscSession>) {
        if self.model != addrs::XAirModel::Xr18 {
            return;
        }
        let task = tokio::spawn(async move {
            let _ = session
                .query_timeout(addrs::SOLO_SW, Duration::from_millis(350))
                .await;
        });
        self.tasks.lock().unwrap().push(task);
    }
    pub(super) fn target_solo_controls(&self) -> Vec<DestinationControl> {
        if self.model != addrs::XAirModel::Xr18 || !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let c = self.target_solo.lock().unwrap();
        if c.generation != generation {
            return vec![];
        }
        let topology = self.topology();
        let Some(all) = (0..self.strip_count())
            .map(|i| openfader_core::strip_key_for_index(&topology, i))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };
        c.rows
            .iter()
            .enumerate()
            .map(|(i, row)| DestinationControl {
                revision: Some(row.revision),
                generation: Some(generation),
                preset: false,
                members: if i == 0 {
                    vec!["main".into()]
                } else {
                    all.clone()
                },
                target: if i == 0 {
                    "main".into()
                } else {
                    format!("vca/{i:02}")
                },
                target_label: if i == 0 {
                    "Main LR".into()
                } else {
                    format!("DCA {i}")
                },
                target_name: None,
                send_slot: None,
                surface: "solo".into(),
                description: "Monitoring Solo switch.".into(),
                id: "solo".into(),
                label: "Solo".into(),
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
                current: row
                    .value
                    .filter(|_| {
                        !row.pending
                            && row
                                .observed
                                .is_some_and(|at| at.elapsed() <= Duration::from_secs(5))
                    })
                    .map(|v| DestinationValue {
                        value: f32::from(v),
                        verified: true,
                    }),
            })
            .collect()
    }
    pub(super) async fn set_target_solo_checked(
        &self,
        target: &str,
        id: &str,
        requested: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let i = target_index(target).ok_or_else(|| anyhow::anyhow!("Unsupported target Solo"))?;
        anyhow::ensure!(
            self.model == addrs::XAirModel::Xr18
                && id == "solo"
                && (requested == 0.0 || requested == 1.0),
            "Invalid target Solo"
        );
        let _lane = self.target_solo_write.lock().await;
        let _dispatch = self.bus_dispatch.read().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let valid = || {
            self.connected.load(Ordering::Acquire)
                && self.connection_generation.load(Ordering::Acquire) == generation
                && check()
        };
        let current = || {
            self.target_solo_controls()
                .into_iter()
                .filter(|r| r.target == target)
                .collect::<Vec<_>>()
        };
        anyhow::ensure!(
            valid()
                && snapshot.len() == 1
                && current() == snapshot
                && snapshot[0]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "Target Solo state or access changed"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        let (_, reply) = session
            .query_received_timeout(addrs::SOLO_SW, Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid() && f32::from(value(&reply, i)?) == expected && current() == snapshot,
            "Target Solo state or access changed"
        );
        {
            let mut c = self.target_solo.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && Some(c.rows[i].revision) == snapshot[0].revision
                    && !c.rows[i].pending,
                "Target Solo changed"
            );
            c.rows[i].pending = true;
            c.rows[i].invalidate();
        }
        let mut pending = Pending {
            driver: self,
            generation,
            index: i,
            confirmed: false,
        };
        let guard = self.session().await?;
        anyhow::ensure!(
            valid()
                && guard
                    .as_ref()
                    .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session)),
            "Target Solo session changed"
        );
        if requested != expected {
            session
                .send_checked(
                    &format!("{}/{:02}", addrs::SOLO_SW, i + 50),
                    vec![OscType::Int(requested as i32)],
                    &|| {
                        anyhow::ensure!(valid(), "Target Solo access or session changed");
                        Ok(())
                    },
                )
                .await?;
        }
        let (at, reply) = session
            .query_received_timeout(addrs::SOLO_SW, Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid() && f32::from(value(&reply, i)?) == requested,
            "Target Solo result uncertain"
        );
        {
            let mut c = self.target_solo.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && c.rows[i].pending
                    && c.rows[i].floor.is_none_or(|floor| at >= floor),
                "Target Solo confirmation invalidated"
            );
            let row = &mut c.rows[i];
            row.value = Some(requested == 1.0);
            row.observed = Some(at);
            row.floor = Some(at);
            row.revision = row.revision.wrapping_add(1);
        }
        anyhow::ensure!(valid(), "Target Solo result uncertain");
        pending.confirmed = true;
        drop(pending);
        current()
            .into_iter()
            .next()
            .filter(|r| {
                r.current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == requested)
            })
            .ok_or_else(|| anyhow::anyhow!("Target Solo session changed"))
    }
}
